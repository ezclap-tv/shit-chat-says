use super::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct TwitchUser {
  id: i32,
  username: String,
  channel_id: Option<i32>,
}
#[derive(Debug)]
pub struct SOAEntry<U, C = i32> {
  channel: Vec<C>,
  chatter: Vec<U>,
  sent_at: Vec<DateTime<Utc>>,
  message: Vec<String>,
}

impl<U, C> SOAEntry<U, C> {
  pub fn new(capacity: usize) -> Self {
    Self {
      channel: Vec::with_capacity(capacity),
      chatter: Vec::with_capacity(capacity),
      sent_at: Vec::with_capacity(capacity),
      message: Vec::with_capacity(capacity),
    }
  }

  pub fn add(&mut self, channel: C, chatter: U, sent_at: DateTime<Utc>, message: String) {
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

  pub fn reserve(&mut self, cap: usize) {
    let extra_cap = cap - self.channel.len().min(cap);
    self.channel.reserve(extra_cap);
    self.chatter.reserve(extra_cap);
    self.sent_at.reserve(extra_cap);
    self.message.reserve(extra_cap);
  }

  #[inline]
  pub fn size(&self) -> usize {
    self.channel.len()
  }

  #[inline]
  pub fn capacity(&self) -> usize {
    self.channel.capacity()
  }
}

pub type ResolvedLogRecord = Entry<i32>;

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct Entry<U> {
  pub id: i64,
  pub channel: U,
  pub chatter: U,
  pub sent_at: DateTime<Utc>,
  pub message: String,
}

impl<U> Clone for Entry<U>
where
  U: Clone,
{
  fn clone(&self) -> Self {
    Self {
      id: self.id,
      channel: self.channel.clone(),
      chatter: self.chatter.clone(),
      sent_at: self.sent_at,
      message: self.message.clone(),
    }
  }
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

pub async fn transfer_raw_logs(db: &crate::Database) -> Result<u64> {
  let mut tx = db.begin().await?;

  sqlx::query(
    "CREATE TEMPORARY TABLE raw_logs_snapshot
  (
      id      bigserial,
      channel varchar,
      chatter varchar,
      sent_at timestamp with time zone,
      message varchar
  );",
  )
  .execute(&mut tx)
  .await?;

  // Create a copy of the raw logs table
  sqlx::query(
    "INSERT INTO raw_logs_snapshot(id, channel, chatter, sent_at, message)
    SELECT id, channel, chatter, sent_at, message
    FROM raw_logs;",
  )
  .execute(&mut tx)
  .await?;

  // Create the user records for each channel and chatter
  sqlx::query(
    "INSERT INTO twitch_user (username, is_logged_as_channel)
    SELECT DISTINCT channel, TRUE
    FROM raw_logs_snapshot
    ON CONFLICT (username) DO NOTHING;",
  )
  .execute(&mut tx)
  .await?;
  sqlx::query(
    "INSERT INTO twitch_user (username)
    SELECT DISTINCT chatter
    FROM raw_logs_snapshot
    ON CONFLICT (username) DO NOTHING;",
  )
  .execute(&mut tx)
  .await?;

  // Insert the raw logs into the main logs table
  let rows = sqlx::query(
    "INSERT INTO twitch_logs (channel, chatter, sent_at, message)
    SELECT channels.id as channel, chatters.id as chatter, rl.sent_at, rl.message
    FROM raw_logs_snapshot rl
             JOIN twitch_user chatters ON chatters.username = rl.chatter
             JOIN twitch_user channels ON channels.username = rl.channel;",
  )
  .execute(&mut tx)
  .await?
  .rows_affected();

  // Clear up the the raw logs table
  sqlx::query(
    "DELETE
    FROM raw_logs
    USING raw_logs_snapshot rl WHERE raw_logs.id = rl.id;",
  )
  .execute(&mut tx)
  .await?;

  // Persist the changes.
  tx.commit().await?;

  Ok(rows)
}

/// Insert a single log entry
pub async fn insert_one(executor: impl sqlx::PgExecutor<'_> + Copy, entry: &Entry<i32>) -> Result<()> {
  let _ = sqlx::query(
    "
    INSERT INTO twitch_logs (channel, chatter, sent_at, message)
    VALUES ($1, $2, $3, $4)
    ",
  )
  .bind(entry.channel)
  .bind(entry.chatter)
  .bind(entry.sent_at)
  .bind(&entry.message)
  .execute(executor)
  .await?;
  Ok(())
}

/// Inserts a batch of logs entries into the raw_logs table. The table doesn't have a primary key, any indexes, or constraints, so inserting data in bulk is extremely quick.
pub async fn insert_soa_raw(
  executor: impl sqlx::PgExecutor<'_> + Copy,
  entry: &mut SOAEntry<String, String>,
) -> Result<()> {
  sqlx::query(
    "
  INSERT INTO raw_logs(channel, chatter, sent_at, message) SELECT * FROM UNNEST($1, $2, $3, $4);",
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

/// Insert log entries in batch mode (efficient for large inserts)
///
/// `entries` will be cleared
pub async fn insert_soa_slow(
  executor: impl sqlx::PgExecutor<'_> + Copy,
  entry: &mut SOAEntry<String, i32>,
) -> Result<()> {
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

/// Insert log entries where the channels and chatters have already been resolved in batch mode
pub async fn insert_soa_resolved(executor: impl sqlx::PgExecutor<'_> + Copy, entry: &mut SOAEntry<i32>) -> Result<()> {
  sqlx::query(
    "
      INSERT INTO twitch_logs (channel, chatter, sent_at, message) SELECT * FROM UNNEST($1, $2, $3, $4);
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
  limit: i32,
  cursor: Option<(i64, DateTime<Utc>)>,
) -> Result<Vec<Entry<String>>> {
  assert!(limit > 0);
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
  query.fetch_all(executor).await
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
  limit: i32,
  cursor: Option<(i64, DateTime<Utc>)>,
) -> Result<Vec<Entry<i32>>> {
  assert!(limit > 0);
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
  query.fetch_all(executor).await
}
