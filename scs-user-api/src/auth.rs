use crate::error::FailWith;
use actix_http::StatusCode;
use actix_web::http::header;
use actix_web::{post, web, FromRequest, HttpResponse, Responder, Result};
use base64::{engine::general_purpose, Engine as _};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use std::{future::Future, pin::Pin};

#[derive(Debug, serde::Deserialize)]
pub struct TokenQuery {
  pub code: String,
  pub redirect_uri: String,
}

#[derive(Debug, serde::Serialize)]
pub struct TokenResponse {
  pub token: AccessToken,
}

#[post("/token")]
pub async fn create_token(
  db: web::Data<db::Database>,
  client: web::Data<reqwest::Client>,
  secret: web::Data<ClientSecret>,
  query: web::Query<TokenQuery>,
) -> Result<impl Responder> {
  use crate::ex::*;

  // request authorization
  log::info!("[authorization] {} {} {}", secret.0, query.code, query.redirect_uri);
  let auth = twitch::id::authorization(&client, &secret.0, &query.code, &query.redirect_uri)
    .await
    .internal()?
    .into_result()
    .with("Invalid authorization code")?;
  // fetch user info based on received token
  log::info!("[get user] {}", auth.access_token);
  let user_info = twitch::helix::get_user(&client, &auth.access_token)
    .await
    .internal()?
    .into_result()
    .internal()?;
  // ensure that user associated with token exists in our DB
  log::info!("[get/create user in db] {} {}", user_info.login, user_info.id);
  let user = db::users::get_or_create(db.get_ref(), &user_info.login, Some(user_info.id.parse().unwrap()))
    .await
    .internal()?;
  // generate a `user-api` access token for them
  let token = AccessToken::generate(user.id());
  log::info!("[generated token] {:?}", token);
  // persist it
  log::info!(
    "[persisted token] {:?}",
    db::tokens::create(
      db.get_ref(),
      user.id(),
      token.token(),
      &auth.access_token,
      &auth.refresh_token,
    )
    .await
    .internal()?
  );
  // then return it to the user
  Ok(HttpResponse::Ok().json(TokenResponse { token }))
}

#[derive(Clone)]
pub struct ClientSecret(pub String);

/// Wrapper over a raw user-api token,
/// and the id of the user associated with that token.
///
/// The token that the UI receives is this pair of values,
/// base64 encoded. This allows us to very easily check
/// the allowlist without making a DB request, just by
/// checking if the `user_id` is present.
#[derive(Debug, Clone, getset::Getters, getset::CopyGetters)]
pub struct AccessToken {
  #[getset(get_copy = "pub")]
  user_id: i32,
  #[getset(get = "pub")]
  token: String,
}

impl AccessToken {
  pub fn generate(user_id: i32) -> Self {
    Self {
      user_id,
      token: thread_rng()
        .sample_iter(&Alphanumeric)
        .take(30)
        .map(char::from)
        .collect(),
    }
  }

  pub fn encode(&self) -> String {
    general_purpose::URL_SAFE.encode(format!("{}-{}", self.user_id, self.token))
  }

  pub fn decode(value: &str) -> Option<Self> {
    let bytes = general_purpose::URL_SAFE.decode(value).ok()?;
    let string = String::from_utf8(bytes).ok()?;
    let (user_id, token) = string
      .split_once('-')
      .and_then(|(id, token)| Some((id.parse::<i32>().ok()?, token.to_string())))?;
    Some(AccessToken { user_id, token })
  }
}

fn bearer_auth_value(v: &header::HeaderValue) -> Option<&str> {
  v.to_str()
    .ok()
    .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
}

impl FromRequest for AccessToken {
  type Error = crate::error::Error;
  type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

  fn from_request(req: &actix_web::HttpRequest, _: &mut actix_http::Payload) -> Self::Future {
    let auth = req
      .headers()
      .get(header::AUTHORIZATION)
      .and_then(bearer_auth_value)
      .and_then(AccessToken::decode);

    let db = req.app_data::<web::Data<db::Database>>().unwrap().clone();
    Box::pin(async move {
      let auth = auth.with(StatusCode::UNAUTHORIZED)?;
      if db::tokens::verify(db.get_ref(), auth.user_id(), &auth.token)
        .await
        .internal()?
      {
        Ok(auth)
      } else {
        Err(StatusCode::UNAUTHORIZED.into())
      }
    })
  }
}

impl serde::Serialize for AccessToken {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    <String as serde::Serialize>::serialize(&self.encode(), serializer)
  }
}

impl<'de> serde::Deserialize<'de> for AccessToken {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    use serde::de::Error;
    let string = <String as serde::Deserialize<'de>>::deserialize(deserializer)?;
    AccessToken::decode(&string).ok_or_else(|| Error::custom("invalid access token"))
  }
}
