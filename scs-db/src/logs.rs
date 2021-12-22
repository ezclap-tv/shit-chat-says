use super::Result;
use crate::users;
use chrono::{DateTime, Utc};
use serde::Serialize;

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

pub type ResolvedEntry = Entry<String>;
pub type RawEntry = Entry<i32>;

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct Entry<U> {
  id: i64,
  channel: U,
  chatter: U,
  sent_at: DateTime<Utc>,
  message: String,
}

impl<U> Entry<U> {
  pub fn new(channel: U, chatter: U, sent_at: DateTime<Utc>, message: String) -> Self {
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
  pub fn channel(&self) -> &U {
    &self.channel
  }

  #[inline]
  pub fn chatter(&self) -> &U {
    &self.chatter
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
pub async fn insert_one(executor: impl sqlx::PgExecutor<'_> + Copy, entry: &Entry<i32>) -> Result<()> {
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
  users::create_bulk(executor, &entry.chatter).await?;

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

macro_rules! get_paged_query {
  (
    $query:ident,
    usernames: $return_usernames:tt,
    $channel:expr,
    $chatter:expr,
    $pattern:expr,
    $limit:expr,
    $cursor:expr,
  ) => {{
    macro_rules! inc {
      ($n:ident) => {{
        $n += 1;
        $n - 1
      }};
    }
    let chatter = $chatter;
    let channel = $channel;
    let pattern = $pattern;
    let limit = $limit;
    let cursor = $cursor;

    let mut n = 1;
    $query = if $return_usernames {
      "SELECT logs.id, tw.username channel, tw2.username chatter, sent_at, message 
       FROM twitch_logs logs\n"
    } else {
      "SELECT * FROM twitch_logs logs\n"
    }
    .to_owned();

    if $return_usernames {
      $query.push_str(
        "JOIN twitch_user tw ON tw.id = logs.channel\n
       JOIN twitch_user tw2 ON tw2.id = logs.chatter\n",
      );
    }

    $query.push_str(&format!(
      "WHERE logs.channel = ({})\n",
      crate::get_channel_id_sql!(inc!(n))
    ));

    if chatter.is_some() {
      $query += &format!("AND logs.chatter = ({})\n", crate::get_channel_id_sql!(inc!(n)));
    }
    if pattern.is_some() {
      $query += &format!("AND logs.message LIKE ${}\n", inc!(n));
    }

    $query += &format!("AND (sent_at, logs.id) < (${}, ${})\n", inc!(n), inc!(n));

    $query += &format!("ORDER BY sent_at DESC, logs.id DESC LIMIT ${}", inc!(n));

    let mut query = sqlx::query_as::<_, Entry<_>>(&$query);

    query = query.bind(channel);
    if let Some(chatter) = chatter {
      query = query.bind(chatter);
    }
    if let Some(pattern) = pattern {
      query = query.bind(format!("%{pattern}%"));
    }

    let (prev_id, prev_sent) = cursor.unwrap_or_else(|| (i64::MAX, chrono::offset::Utc::now()));
    query = query.bind(prev_sent);
    query = query.bind(prev_id);
    query = query.bind(limit);

    query
  }};
}

pub async fn fetch_logs_paged_with_usernames<S: Into<String>>(
  executor: impl sqlx::PgExecutor<'_> + Copy,
  channel: S,
  chatter: Option<S>,
  pattern: Option<S>,
  limit: u32,
  cursor: Option<(i64, DateTime<Utc>)>,
) -> Result<Vec<Entry<String>>> {
  let mut query;
  let query = get_paged_query!(
    query,
    usernames: true,
    channel.into(),
    chatter.map(|v| v.into()),
    pattern.map(|v| v.into()),
    limit,
    cursor,
  );
  Ok(query.fetch_all(executor).await?)
}

/// Retrieve logs into a `Vec`
///
/// * channel - exact
/// * chatter - exact
/// * pattern - uses `LIKE` for matching, e.g. `%yo%`
///   * `%` multi-character wildcard
///   * `_` single-character wildcard
pub async fn fetch_logs_paged<S: Into<String>>(
  executor: impl sqlx::PgExecutor<'_>,
  channel: S,
  chatter: Option<S>,
  pattern: Option<S>,
  limit: u32,
  cursor: Option<(i64, DateTime<Utc>)>,
) -> Result<Vec<Entry<i32>>> {
  let mut query;
  let query = get_paged_query!(
    query,
    usernames: false,
    channel.into(),
    chatter.map(|v| v.into()),
    pattern.map(|v| v.into()),
    limit,
    cursor,
  );
  Ok(query.fetch_all(executor).await?)
}
