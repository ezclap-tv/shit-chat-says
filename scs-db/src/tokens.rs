use super::Result;

#[derive(Debug, sqlx::FromRow, getset::Getters)]
#[getset(get = "pub")]
pub struct Token {
  user_id: i32,
  twitch_access_token: String,
  twitch_refresh_token: String,
  scs_user_api_token: String,
}

pub async fn create(
  executor: impl sqlx::PgExecutor<'_> + Copy,
  // We require `user_id` instead of `username` because the user must already exist
  user_id: i32,
  scs_user_api_token: &str,
  twitch_access_token: &str,
  twitch_refresh_token: &str,
) -> Result<Token> {
  sqlx::query_as::<_, Token>(
    "
      INSERT INTO tokens (user_id, scs_user_api_token, twitch_access_token, twitch_refresh_token)
        VALUES ($1, $2, $3, $4)
        RETURNING *
      ",
  )
  .bind(user_id)
  .bind(scs_user_api_token)
  .bind(twitch_access_token)
  .bind(twitch_refresh_token)
  .fetch_one(executor)
  .await
}

pub async fn delete(executor: impl sqlx::PgExecutor<'_> + Copy, scs_user_api_token: &str) -> Result<()> {
  sqlx::query(
    "
    DELETE FROM tokens
      WHERE scs_user_api_token = $1
    ",
  )
  .bind(scs_user_api_token)
  .execute(executor)
  .await?;
  Ok(())
}

pub async fn verify(
  executor: impl sqlx::PgExecutor<'_> + Copy,
  user_id: i32,
  scs_user_api_token: &str,
) -> Result<bool> {
  sqlx::query_scalar::<_, bool>(
    "
      SELECT TRUE
      WHERE (
        SELECT TRUE FROM allowlist
          WHERE id = $1
      )
      AND (
        SELECT TRUE FROM tokens
          WHERE user_id = $1
          AND scs_user_api_token = $2
      )
      ",
  )
  .bind(user_id)
  .bind(scs_user_api_token)
  .fetch_optional(executor)
  .await
  .map(|v| v.unwrap_or(false))
}
