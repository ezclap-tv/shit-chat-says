use std::borrow::Cow;

use async_trait::async_trait;

use crate::sink::SinkError;

/// Specifies what table our logs should be written to.
#[derive(Debug, Clone, Copy)]
pub enum TargetTable {
  RawLogs,
  IndexedLogs,
}

pub struct PostgresSink {
  target: TargetTable,
  last_flushed: std::time::Instant,
  max_size: usize,
  buf: db::logs::SOAEntry<String, String>,
  db: db::Database,
}

impl PostgresSink {
  pub fn new(db: db::Database, buf_size: usize, target: TargetTable) -> Self {
    Self {
      target,
      last_flushed: std::time::Instant::now(),
      max_size: buf_size,
      db,
      buf: db::logs::SOAEntry::new(buf_size),
    }
  }
}

#[async_trait]
impl crate::Sink for PostgresSink {
  fn name(&self) -> Cow<'static, str> {
    Cow::Borrowed(match self.target {
      TargetTable::RawLogs => "db:raw_logs",
      TargetTable::IndexedLogs => "db:indexed_logs",
    })
  }

  async fn handle_messages(&mut self, batch: Vec<crate::sink::RawLogRecord>) -> Result<(), SinkError> {
    // This shouldn't ever happen.
    if batch.is_empty() {
      return Ok(());
    }

    for msg in batch {
      self.buf.add(
        msg.channel.to_string(),
        msg.chatter.to_string(),
        msg.sent_at,
        msg.message,
      );
    }

    if self.buf.size() >= self.max_size {
      self.flush().await?;
    }

    Ok(())
  }

  async fn flush(&mut self) -> Result<(), SinkError> {
    if self.buf.size() == 0 {
      return Ok(());
    }
    let rows = match self.target {
      TargetTable::RawLogs => db::logs::insert_soa_raw(&self.db, &mut self.buf).await?,
      TargetTable::IndexedLogs => db::logs::insert_soa_slow(&self.db, &mut self.buf).await?,
    };
    self.last_flushed = std::time::Instant::now();
    log::info!("Inserted {rows} row(s) into the database");
    Ok(())
  }

  async fn must_flush(&mut self) -> Result<(), SinkError> {
    self.flush().await
  }
}
