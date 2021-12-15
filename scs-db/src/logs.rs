use super::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct TwitchUser {
  id: i32,
  username: String,
  channel_id: Option<i32>,
}
pub struct SOAEntry {
  channel: Vec<i32>,
  chatter: Vec<String>,
  sent_at: Vec<DateTime<Utc>>,
  message: Vec<String>,
}

impl SOAEntry {
  pub fn new(capacity: usize) -> Self {
    Self {
      channel: Vec::with_capacity(capacity),
      chatter: Vec::with_capacity(capacity),
      sent_at: Vec::with_capacity(capacity),
      message: Vec::with_capacity(capacity),
    }
  }

  pub fn add(&mut self, channel: i32, chatter: String, sent_at: DateTime<Utc>, message: String) {
    self.channel.push(channel);
    self.chatter.push(chatter);
    self.sent_at.push(sent_at);
    self.message.push(message);
  }

  pub fn clear(&mut self) {
    self.channel.clear();
    self.chatter.clear();
    self.sent_at.clear();
    self.message.clear();
  }
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct Entry {
  id: i64,
  channel: i32,
  chatter: i32,
  sent_at: DateTime<Utc>,
  message: String,
}

impl Entry {
  pub fn new(channel: i32, chatter: i32, sent_at: DateTime<Utc>, message: String) -> Entry {
    Entry {
      id: -1,
      channel,
      chatter,
      sent_at,
      message,
    }
  }

  #[inline]
  pub fn is_valid(&self) -> bool {
    self.id > -1
  }

  #[inline]
  pub fn id(&self) -> i64 {
    self.id
  }

  #[inline]
  pub fn channel(&self) -> i32 {
    self.channel
  }

  #[inline]
  pub fn chatter(&self) -> i32 {
    self.chatter
  }

  #[inline]
  pub fn sent_at(&self) -> &DateTime<Utc> {
    &self.sent_at
  }

  #[inline]
  pub fn message(&self) -> &str {
    &self.message
  }
}

/// Insert a single log entry
pub async fn insert_one(executor: impl sqlx::PgExecutor<'_> + Copy, entry: &Entry) -> Result<()> {
  let _ = sqlx::query(
    "
    INSERT INTO twitch_logs (channel, chatter, sent_at, message)
    VALUES ($1, $2, $3, $4)
    ",
  )
  .bind(&entry.channel)
  .bind(&entry.chatter)
  .bind(&entry.sent_at)
  .bind(&entry.message)
  .execute(executor)
  .await?;
  Ok(())
}

/// Insert log entries in batch mode (efficient for large inserts)
///
/// `entries` will be cleared
pub async fn insert_soa(executor: impl sqlx::PgExecutor<'_> + Copy, entry: &mut SOAEntry) -> Result<()> {
  // Bulk insert the chatters
  sqlx::query(
    "
    INSERT INTO twitch_user (username)
	    SELECT * FROM UNNEST($1)
    ON CONFLICT (username) DO NOTHING;
    ",
  )
  .bind(&entry.chatter)
  .execute(executor)
  .await?;

  // Then complete the insert into logs by joining chatters with twitch_user
  sqlx::query(
    "
    WITH raw_logs AS (
      SELECT * 
      FROM UNNEST($1, $2, $3, $4) 
      soa_entry(channel, chatter, sent_at, message)
    ) 
    INSERT INTO twitch_logs (channel, chatter, sent_at, message)
    SELECT * FROM (
      SELECT rl.channel, tw.id chatter, rl.sent_at, rl.message
      FROM raw_logs rl
      JOIN twitch_user tw ON tw.username = rl.chatter
    ) as joined;
    ",
  )
  .bind(&entry.channel)
  .bind(&entry.chatter)
  .bind(&entry.sent_at)
  .bind(&entry.message)
  .execute(executor)
  .await?;

  entry.clear();

  Ok(())
}

/// Retrieve logs into a `Vec`
///
/// * channel - exact
/// * chatter - exact
/// * pattern - uses `LIKE` for matching, e.g. `%yo%`
///   * `%` multi-character wildcard
///   * `_` single-character wildcard
pub async fn fetch_all<S: Into<String>>(
  executor: impl sqlx::PgExecutor<'_>,
  channel: S,
  chatter: Option<S>,
  pattern: Option<S>,
  offset: Option<i32>,
  limit: Option<i32>,
) -> Result<Vec<Entry>> {
  macro_rules! inc {
    ($n:ident) => {{
      $n += 1;
      $n - 1
    }};
  }
  let mut n = 1i32;

  let mut query = format!(
    "SELECT * FROM twitch_logs WHERE channel = ({})\n",
    crate::get_channel_id_sql!(inc!(n))
  );
  if chatter.is_some() {
    query += &format!("AND chatter = ({})\n", crate::get_channel_id_sql!(inc!(n)));
  }
  if pattern.is_some() {
    query += &format!("AND message LIKE ${}\n", inc!(n));
  }
  query += &format!(
    "LIMIT ${} OFFSET ${}",
    if limit.is_some() {
      inc!(n).to_string()
    } else {
      "ALL".to_string()
    },
    inc!(n)
  );

  let mut query = sqlx::query_as::<_, Entry>(&query);

  query = query.bind(channel.into());
  if let Some(chatter) = chatter {
    query = query.bind(chatter.into());
  }
  if let Some(pattern) = pattern {
    query = query.bind(pattern.into());
  }
  if let Some(limit) = limit {
    query = query.bind(limit);
  }
  query = query.bind(offset.unwrap_or(0));

  Ok(query.fetch_all(executor).await?)
}
