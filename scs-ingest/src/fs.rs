use std::{
  borrow::Cow,
  collections::HashMap,
  path::{Path, PathBuf},
};

use anyhow::Context;
use async_trait::async_trait;
use chrono::Utc;
use db::sqlx::types::chrono::DateTime;
use smol_str::SmolStr;
use tokio::io::{AsyncWriteExt, BufWriter};

use crate::{sink::SinkError, Channel};

pub struct FileSystemSink {
  scratchpad: HashMap<SmolStr, Vec<u8>>,
  sinks: HashMap<SmolStr, DailyLogSink>,
}

impl FileSystemSink {
  pub async fn new(channels: Vec<Channel>, output_directory: &Path) -> Result<Self, SinkError> {
    let scratchpad = channels
      .iter()
      .map(|c| (c.name.clone(), Vec::with_capacity(c.buffer)))
      .collect();
    let futures = channels.into_iter().map(|channel| async move {
      log::info!("Initializing log file for {}", channel.name);

      let sink = DailyLogSink::new(output_directory.clone(), channel.name.clone(), channel.buffer).await;
      sink.map(|sink| (channel.name, sink))
    });
    let sinks = futures::future::try_join_all(futures)
      .await?
      .into_iter()
      .collect::<HashMap<_, _>>();

    Ok(Self { sinks, scratchpad })
  }

  fn report_and_return_last_error(results: Vec<Result<(), anyhow::Error>>) -> Result<(), SinkError> {
    let mut last_error = None;
    for r in results {
      if let Err(e) = r {
        log::error!("Error while writing to sink: {:?}", e);
        last_error = Some(e);
      }
    }
    last_error.map_or(Ok(()), |e| Err(SinkError::from(e)))
  }
}

#[async_trait]
impl crate::Sink for FileSystemSink {
  fn name(&self) -> Cow<'static, str> {
    Cow::Borrowed("filesystem")
  }

  async fn handle_messages(&mut self, batch: Vec<crate::sink::RawLogRecord>) -> Result<(), SinkError> {
    // This shouldn't ever happen.
    if batch.is_empty() {
      return Ok(());
    }

    self.scratchpad.values_mut().for_each(|buf| buf.clear());

    for record in batch {
      let scratch = self
        .scratchpad
        .get_mut(record.channel())
        .expect("Received message from an unknown channel");

      scratch.extend_from_slice(record.sent_at.to_rfc3339().as_bytes());
      scratch.push(b',');
      scratch.extend_from_slice(record.chatter.as_bytes());
      scratch.push(b',');
      scratch.extend_from_slice(record.message.as_bytes());
      scratch.push(b'\n');
    }

    let writes = self.sinks.iter_mut().filter_map(|(channel, sink)| {
      self
        .scratchpad
        .get(channel)
        .and_then(|buf| if buf.is_empty() { None } else { Some(buf) })
        .map(|buf| sink.write(buf))
    });
    let results = futures::future::join_all(writes).await;
    Self::report_and_return_last_error(results)
  }

  async fn flush(&mut self) -> Result<(), SinkError> {
    futures::future::try_join_all(self.sinks.values_mut().map(DailyLogSink::flush)).await?;
    Ok(())
  }

  async fn must_flush(&mut self) -> Result<(), SinkError> {
    let results = futures::future::join_all(self.sinks.values_mut().map(DailyLogSink::flush)).await;
    Self::report_and_return_last_error(results)
  }
}

/// File sink which writes to a new file for each day
#[derive(Debug)]
pub struct DailyLogSink {
  log_file_prefix: SmolStr,
  log_dir: PathBuf,
  date: DateTime<Utc>,
  file: BufWriter<tokio::fs::File>,
}

async fn open_log_file(dir: &Path, prefix: &str) -> anyhow::Result<tokio::fs::File> {
  let date = Utc::now().format("%F");
  let log_file_path = dir.join(format!("{prefix}-{date}.log"));
  tokio::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(&log_file_path)
    .await
    .with_context(|| format!("Error while opening log file for {}", log_file_path.display()))
}

impl DailyLogSink {
  pub async fn new(log_dir: &Path, log_file_prefix: SmolStr, buf_size: usize) -> anyhow::Result<Self> {
    let log_dir = log_dir.join(log_file_prefix.as_str());
    if !log_dir.exists() {
      tokio::fs::create_dir_all(&log_dir).await?;
    }
    let date = Utc::now();
    let file = open_log_file(&log_dir, &log_file_prefix)
      .await
      .map(|file| BufWriter::with_capacity(buf_size, file))?;

    Ok(DailyLogSink {
      log_file_prefix,
      log_dir,
      date,
      file,
    })
  }

  pub async fn write(&mut self, buf: &[u8]) -> anyhow::Result<()> {
    // rotate file every day
    let today = Utc::now();
    if today.signed_duration_since(self.date).num_days() > 0 {
      self.date = today;
      self.file.flush().await?;
      *self.file.get_mut() = open_log_file(&self.log_dir, &self.log_file_prefix).await?;
    }
    // then actually write
    self.file.write_all(buf).await?;
    Ok(())
  }

  pub async fn flush(&mut self) -> anyhow::Result<()> {
    self.file.flush().await?;
    Ok(())
  }
}
