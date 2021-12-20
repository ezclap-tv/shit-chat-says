use crate::error::IntoActixResult;
use actix_web::{post, web, HttpResponse, Responder, Result};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::Deserialize;

pub const CLIENT_ID: &str = "0ncr6cfrybexz4ivgtd1kmpq0lq5an";

#[derive(Debug, Deserialize)]
pub struct TokenQuery {
  pub code: String,
  pub redirect_uri: String,
}

fn token_url(client_id: &str, client_secret: &str, code: &str, redirect_uri: &str) -> String {
  format!(
    "\
    https://id.twitch.tv/oauth2/token\
      ?client_id={client_id}\
      &client_secret={client_secret}\
      &code={code}\
      &grant_type=authorization_code\
      &redirect_uri={redirect_uri}\
  "
  )
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TwitchTokenResponse {
  Success {
    access_token: String,
    refresh_token: String,
    expires_in: usize,
    token_type: String,
    scope: Vec<String>,
  },
  Error {
    status: u16,
    message: String,
  },
}

#[post("/token")]
pub async fn create_token(
  client: web::Data<reqwest::Client>,
  secret: web::Data<ClientSecret>,
  query: web::Query<TokenQuery>,
) -> Result<impl Responder> {
  let next = token_url(CLIENT_ID, &secret.0, &query.code, &query.redirect_uri);
  log::info!("url: {}", next);

  use TwitchTokenResponse::*;
  match client
    .post(next)
    .send()
    .await
    .to_actix()?
    .json::<TwitchTokenResponse>()
    .await
    .to_actix()?
  {
    Success {
      access_token,
      refresh_token,
      expires_in,
      token_type,
      scope,
    } => {
      log::info!(
        "Got: access_token {}, refresh_token {}, expires_in {}, token_type {}, scope [{}]",
        access_token,
        refresh_token,
        expires_in,
        token_type,
        scope.join(",")
      );
      // TODO:
      // generate token
      // store in DB so that we can confirm user's identity in the future
      // return generated token in response
      Ok(HttpResponse::Ok().finish())
    }
    Error { status, message } => {
      log::info!("Failed with {} {}", status, message);
      Ok(HttpResponse::BadRequest().body("Invalid code"))
    }
  }
}

#[derive(Clone)]
pub struct ClientSecret(pub String);

pub fn generate_token() -> String {
  thread_rng()
    .sample_iter(&Alphanumeric)
    .take(15)
    .map(char::from)
    .collect()
}
