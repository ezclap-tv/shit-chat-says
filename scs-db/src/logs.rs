use chrono::{DateTime, Utc};

#[derive(Debug, sqlx::FromRow)]
pub struct Entry {
  id: i32,
  channel: String,
  chatter: String,
  sent_at: DateTime<Utc>,
  message: String,
}

macro_rules! inc {
  ($n:ident) => {{
    $n += 1;
    $n - 1
  }};
}

impl Entry {
  pub fn new(channel: String, chatter: String, sent_at: DateTime<Utc>, message: String) -> Entry {
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
  pub fn id(&self) -> i32 {
    self.id
  }
  #[inline]
  pub fn channel(&self) -> &str {
    &self.channel
  }
  #[inline]
  pub fn chatter(&self) -> &str {
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

  /// Insert a single log entry
  pub async fn insert_one(executor: impl sqlx::PgExecutor<'_> + Copy, entry: &Entry) -> sqlx::Result<()> {
    let _ = sqlx::query(
      "
    INSERT INTO logs (channel, chatter, sent_at, message)
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
  pub async fn insert_soa(executor: impl sqlx::PgExecutor<'_> + Copy, entries: Vec<Entry>) -> sqlx::Result<()> {
    let (mut channel, mut chatter, mut sent_at, mut message) = (
      Vec::<String>::with_capacity(entries.len()),
      Vec::<String>::with_capacity(entries.len()),
      Vec::<DateTime<Utc>>::with_capacity(entries.len()),
      Vec::<String>::with_capacity(entries.len()),
    );

    for entry in entries.into_iter() {
      channel.push(entry.channel);
      chatter.push(entry.chatter);
      sent_at.push(entry.sent_at);
      message.push(entry.message);
    }

    sqlx::query(
      "
    INSERT INTO logs (channel, chatter, sent_at, message)
    SELECT * FROM UNNEST($1, $2, $3, $4)
    ",
    )
    .bind(&channel)
    .bind(&chatter)
    .bind(&sent_at)
    .bind(&message)
    .execute(executor)
    .await?;

    Ok(())
  }

  /// * channel - exact
  /// * chatter - exact
  /// * pattern - uses `LIKE` for matching, e.g. `%yo%`
  ///   * `%` multi-character wildcard
  ///   * `_` single-character wildcard
  pub async fn find_all(
    executor: impl sqlx::PgExecutor<'_> + Copy,
    channel: String,
    chatter: Option<String>,
    pattern: Option<String>,
    offset: i32,
    limit: i32,
  ) -> sqlx::Result<Vec<Entry>> {
    let mut n = 0;
    let mut query = format!("SELECT * FROM logs WHERE channel = ${}\n", inc!(n));
    if chatter.is_some() {
      query += &format!("AND chatter = ${}\n", inc!(n));
    }
    if pattern.is_some() {
      query += &format!("AND message LIKE ${}\n", inc!(n));
    }
    query += &format!("LIMIT {} OFFSET {}", inc!(n), inc!(n));

    let mut query = sqlx::query_as::<_, Entry>(&query);

    query = query.bind(&channel);
    if let Some(chatter) = &chatter {
      query = query.bind(chatter);
    }
    if let Some(pattern) = &pattern {
      query = query.bind(pattern);
    }
    query = query.bind(limit);
    query = query.bind(offset);

    query.fetch_all(executor).await
  }
}
