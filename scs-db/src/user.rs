use super::Result;

#[derive(Debug, sqlx::FromRow, getset::Getters, getset::CopyGetters)]
pub struct TwitchUser {
  #[getset(get_copy = "pub")]
  id: i32,
  #[getset(get = "pub")]
  username: String,
  #[getset(get_copy = "pub")]
  is_logged_as_channel: bool,
  #[getset(get = "pub")]
  channel_id: Option<i32>,
}

impl TwitchUser {
  pub async fn get_or_create(
    executor: impl sqlx::PgExecutor<'_> + Copy,
    username: &str,
    channel_id: Option<i32>,
  ) -> Result<TwitchUser> {
    Ok(
      sqlx::query_as::<_, TwitchUser>(
        "
        WITH
        selected AS (
           SELECT * FROM twitch_user
           WHERE username = $1
        ),
        inserted AS (
           INSERT INTO twitch_user (username)
            VALUES ($1)
           ON CONFLICT (username) DO NOTHING
           RETURNING *
        )
        SELECT * FROM selected
        UNION ALL
        SELECT * FROM inserted
        ",
      )
      .bind(username)
      .bind(channel_id)
      .fetch_one(executor)
      .await?,
    )
  }

  pub async fn create_bulk(executor: impl sqlx::PgExecutor<'_> + Copy, usernames: &[String]) -> Result<()> {
    sqlx::query(
      "
      INSERT INTO twitch_user (username)
        SELECT * FROM UNNEST($1)
      ON CONFLICT (username) DO NOTHING;
      ",
    )
    .bind(usernames)
    .execute(executor)
    .await?;
    Ok(())
  }
}
