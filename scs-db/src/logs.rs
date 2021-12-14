use super::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct TwitchUser {
  id: i32,
  username: String,
  channel_id: Option<i32>,
}

/// Must not be shared between columns
pub struct SOAChannel {
  username_to_id: ahash::AHashMap<String, i32>,
  users: Vec<i32>,
}

impl SOAChannel {
  pub fn new(capacity: usize) -> Self {
    Self {
      username_to_id: ahash::AHashMap::with_capacity(capacity),
      users: Vec::with_capacity(capacity),
    }
  }

  #[inline]
  pub fn get_temporary_id(&mut self, channel: &str) -> i32 {
    // Mutable borrow problem
    let mut users = std::mem::take(&mut self.users);

    let id = *self
      .username_to_id
      .raw_entry_mut()
      .from_key(channel)
      .or_insert_with(|| {
        assert!(users.len() < i32::max_value() as usize);
        users.push(0);
        (channel.to_owned(), users.len() as i32 - 1)
      })
      .1;

    self.users = users;
    id
  }

  pub async fn resolve_temporary_ids(
    &mut self,
    executor: impl sqlx::PgExecutor<'_> + Copy,
    column: &mut Vec<i32>,
  ) -> Result<()> {
    let mut usernames = self.username_to_id.keys().cloned().collect::<Vec<_>>();
    usernames.sort();

    //let inserted = sqlx::query_as::<_, (i32, String)>(
    let mut inserted = vec![];
    let mut count = 0;
    while inserted.len() != usernames.len() && count < 3 {
      inserted = sqlx::query_scalar::<_, i32>(
        r#"
    WITH input_rows as (
      SELECT * FROM UNNEST($1) username
   ), inserted as (
     INSERT INTO twitch_user (username)
       SELECT * FROM input_rows
     ON CONFLICT (username) DO NOTHING
       RETURNING id
    )
     SELECT id, tw.username FROM inserted JOIN twitch_user tw USING (id)
     UNION ALL
     SELECT
       c.id, username
     FROM
       input_rows
       JOIN twitch_user c USING (username)
     ORDER BY username;   
    "#,
      )
      .bind(&usernames)
      .fetch_all(executor)
      .await?;

      if count > 1 {
        log::error!(
          "inserted.len() != usernames.len() ({} != {}), have to refetch (?)",
          inserted.len(),
          usernames.len()
        );
      }

      count += 1;
    }

    if inserted.len() != usernames.len() {
      assert_eq!(
        inserted.len(),
        usernames.len(),
        "Failed to insert/get all usernames after {count} attempts",
      );
      // anyhow::bail!(format!(
      //   "Failed to insert/get all usernames after {} attempts (inserted != usernames: {} != {})",
      //   count,
      //   inserted.len(),
      //   usernames.len(),
      // ));
    }

    for (order, username) in usernames.into_iter().enumerate() {
      let temporary_id = *self.username_to_id.get(&username).unwrap();
      *self.users.get_mut(temporary_id as usize).unwrap() = inserted[order];
    }

    for value in column {
      *value = self.users[*value as usize];
    }

    self.username_to_id.clear();
    self.users.clear();
    Ok(())
  }
}

pub struct SOAEntry {
  channel: Vec<i32>,
  pub chatter: Vec<i32>,
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

  pub fn add(&mut self, channel: i32, chatter: i32, sent_at: DateTime<Utc>, message: String) {
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
pub async fn insert_soa(executor: impl sqlx::PgExecutor<'_>, entry: &mut SOAEntry) -> Result<()> {
  sqlx::query(
    "
    INSERT INTO twitch_logs (channel, chatter, sent_at, message)
    SELECT * FROM UNNEST($1, $2, $3, $4)
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
