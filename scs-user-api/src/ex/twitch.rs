use anyhow::Context;

pub const CLIENT_ID: &str = "0ncr6cfrybexz4ivgtd1kmpq0lq5an";

#[derive(Debug, serde::Deserialize)]
pub struct Error {
  pub status: u16,
  pub message: String,
}

impl std::fmt::Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.message)
  }
}

impl std::error::Error for Error {}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum Response<T> {
  Success(T),
  Error(Error),
}

impl<T> Response<T> {
  pub fn map<U>(self, f: impl Fn(T) -> U) -> Response<U> {
    use Response::*;
    match self {
      Success(v) => Success(f(v)),
      Error(e) => Error(e),
    }
  }

  pub fn into_result(self) -> anyhow::Result<T> {
    use Response::*;
    match self {
      Success(v) => anyhow::Result::Ok(v),
      Error(e) => anyhow::Result::Err(anyhow::Error::from(e)),
    }
  }
}

pub mod id {
  use super::*;

  #[derive(Debug, serde::Deserialize)]
  pub struct Authorization {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: usize,
    pub token_type: String,
    pub scope: Vec<String>,
  }

  pub async fn authorization(
    client: &reqwest::Client,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
  ) -> anyhow::Result<Response<Authorization>> {
    Ok(
      client
        .post(format!(
          "\
          https://id.twitch.tv/oauth2/token\
            ?client_id={CLIENT_ID}\
            &client_secret={client_secret}\
            &code={code}\
            &grant_type=authorization_code\
            &redirect_uri={redirect_uri}\
          "
        ))
        .send()
        .await?
        .json()
        .await?,
    )
  }
}

pub mod helix {
  use super::*;

  /// Twitch API (Helix) puts response data in an array under the key `data`
  #[derive(Debug, serde::Deserialize)]
  pub struct Data<T> {
    pub data: Vec<T>,
  }

  #[derive(Debug, serde::Deserialize)]
  pub struct GetUser {
    pub broadcaster_type: String,
    pub description: String,
    pub display_name: String,
    pub id: String,
    pub login: String,
    pub offline_image_url: String,
    pub profile_image_url: String,
    pub r#type: String,
    pub view_count: i64,
    pub email: String,
    pub created_at: String,
  }

  pub async fn get_user(client: &reqwest::Client, token: &str) -> anyhow::Result<Response<GetUser>> {
    let res = client
      .get("https://api.twitch.tv/helix/users")
      .bearer_auth(token)
      .header("Client-Id", CLIENT_ID)
      .send()
      .await
      .context("Failed to fetch")?
      .json::<Response<Data<GetUser>>>()
      .await
      .context("Failed to deserialize")?;
    Ok(res.map(|mut v| v.data.swap_remove(0)))
  }
}
