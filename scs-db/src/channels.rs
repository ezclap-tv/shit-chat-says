use super::Result;

pub async fn get_logged_channels(executor: impl sqlx::PgExecutor<'_>) -> Result<Vec<String>> {
  // TODO: be move careful with this one if we start logging more channels
  Ok(
    sqlx::query_scalar::<_, String>("SELECT username FROM twitch_user WHERE is_logged_as_channel = true")
      .fetch_all(executor)
      .await?,
  )
}

#[macro_export]
macro_rules! get_channel_id_sql {
  ($parameter:expr) => {
    format!("SELECT id FROM twitch_user WHERE username = ${}", $parameter)
  };
}

pub async fn get_channel_id(executor: impl sqlx::PgExecutor<'_>, username: &str) -> Result<i32> {
  Ok(
    sqlx::query_scalar::<_, i32>(&get_channel_id_sql!("1"))
      .bind(username)
      .fetch_one(executor)
      .await?,
  )
}

pub async fn get_or_create_channel(
  executor: impl sqlx::PgExecutor<'_> + Copy,
  username: &str,
  is_logged_as_channel: bool,
  cache: &mut ahash::AHashMap<String, i32>,
) -> Result<i32> {
  // Fast path: username is in the local cache
  if let Some(id) = cache.get(username).copied() {
    return Ok(id);
  }

  // Slow path: user is either in the database or doesn't exist
  // This seems to be 30% faster than doing SELECT + INSERT
  let id = sqlx::query_scalar::<_, i32>(
    r#"
  WITH input_rows (
    username, is_logged_as_channel
  ) AS (
    VALUES ($1, $2)
  ),
  inserted AS (
  INSERT INTO twitch_user (username, is_logged_as_channel)
    SELECT * FROM input_rows
    ON CONFLICT (username) 
      DO UPDATE 
        SET is_logged_as_channel = EXCLUDED.is_logged_as_channel 
        WHERE twitch_user.username = EXCLUDED.username 
        AND twitch_user.is_logged_as_channel = FALSE
    RETURNING id, is_logged_as_channel
  )
  SELECT id, is_logged_as_channel FROM inserted
  UNION ALL
  SELECT
    c.id, c.is_logged_as_channel
  FROM
    input_rows
    JOIN twitch_user c USING (username);
  "#,
  )
  .bind(username)
  .bind(&is_logged_as_channel)
  .fetch_one(executor)
  .await?;

  cache.insert(username.to_owned(), id);
  Ok(id)
}
