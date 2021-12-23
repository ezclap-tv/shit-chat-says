use chrono::DateTime;
use signal_hook::{
  consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM},
  iterator::Signals,
};
use std::{
  fs,
  io::{BufWriter, Read, Write},
  path::{Path, PathBuf},
  sync::Arc,
  time::Duration,
};

// 512 bytes per message max
pub const TWITCH_MESSAGE_SIZE: usize = 512;
pub const USERNAME_CACHE_SIZE: usize = 5_000; // 250kb per channel max, should probably be configurable

pub type MsgRx = tokio::sync::mpsc::UnboundedReceiver<db::logs::UnresolvedEntry>;
pub type MsgTx = tokio::sync::mpsc::UnboundedSender<db::logs::UnresolvedEntry>;

#[derive(Debug)]
enum FsBufMsg {
  Write(db::logs::UnresolvedEntry),
  Rotate(String),
  Stop,
}

#[derive(Debug)]
struct FsMirrorBuffer {
  rx: crossbeam_channel::Receiver<FsBufMsg>,
  sinks: ahash::AHashMap<String, BackupLogSink>,
}
impl FsMirrorBuffer {
  fn new(rx: crossbeam_channel::Receiver<FsBufMsg>, sinks: ahash::AHashMap<String, BackupLogSink>) -> Self {
    Self { rx, sinks }
  }

  fn spawn_writer_thread(self) -> std::thread::JoinHandle<std::io::Result<()>> {
    let me = Arc::new(tokio::sync::Mutex::new(self));

    let buf_ref = me.clone();
    let handle = std::thread::spawn(move || {
      log::info!("[FS_MIRROR] Signal thread spawned");
      let mut signals = Signals::new(&[SIGHUP, SIGTERM, SIGINT, SIGQUIT])?;

      'outer: loop {
        // We just want to consume a signal -- any signal -- and immediately terminate the spin loop.
        #[allow(clippy::never_loop)]
        for _ in signals.pending() {
          break 'outer;
        }
        std::hint::spin_loop();
      }

      log::info!("[FS_MIRROR] Received a stop signal, shutting down...");

      // After a signal was received, flush the sinks to the filesystem
      let mut try_unwrap_count = 0;
      let mut buf_ref = buf_ref;
      let buf = loop {
        log::info!("[FS_MIRROR] Trying to obtain the buffer");
        match std::sync::Arc::try_unwrap(buf_ref) {
          Ok(buf) => break buf,
          Err(e) => {
            buf_ref = e;
            try_unwrap_count += 1
          }
        }
        if try_unwrap_count > 10 {
          panic!("Couldn't obtain ownership of the fs buffer after 10 retries -- did the other thread exit after receiving the signal?")
        }

        log::info!("[FS_MIRROR] Attempt failed. Waiting for 100ms...");

        // Wait for the 100s for the other thread to abort()
        std::thread::sleep(std::time::Duration::from_millis(100));
      };

      log::info!("[FS_MIRROR] Buffer obtained: {:?}", buf);

      let buf = buf.into_inner();
      for (name, sink) in buf.sinks {
        log::info!("[FS_MIRROR] Flushing {}", name);
        let buffer = sink.file.buffer().to_owned();
        let mut file = sink
          .file
          .into_inner()
          .expect("This cannot happen as the other thread must have terminated for us to get here.");

        match file.write_all(&buffer) {
          Ok(_) => (),
          Err(e) => {
            eprintln!("Failed to flush one of the sinks: {e}");
          }
        }
      }

      std::io::Result::Ok(())
    });

    std::thread::spawn(move || {
      log::info!("[FS_MIRROR] Writer thread spawned");

      let mut me = me
        .try_lock()
        .expect("No other thread is holding the lock, so this can't fail");

      me.handle_write_messages();
    });

    handle
  }

  fn handle_write_messages(&mut self) {
    const MAX_ERROR_COUNT: usize = 32;
    let mut error_count = 0;

    macro_rules! on_err {
      ($count:ident, $expr:expr) => {
        match $expr {
          Ok(value) => value,
          Err(e) => {
            log::error!("Failed to perform a write: {}", e);
            $count += 1;
            if $count > MAX_ERROR_COUNT {
              break;
            }
            continue;
          }
        }
      };
    }
    loop {
      match self.rx.recv_timeout(Duration::from_millis(100)) {
        Ok(msg) => match msg {
          FsBufMsg::Write(entry) => {
            let sink = self.sinks.get_mut(entry.channel()).unwrap();
            on_err!(error_count, sink.write_message(entry));
          }
          FsBufMsg::Rotate(entry) => {
            let sink = self.sinks.get_mut(&entry).unwrap();
            on_err!(error_count, sink.rotate());
          }
          FsBufMsg::Stop => break,
        },
        Err(crossbeam_channel::RecvTimeoutError::Timeout) => (),
        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
      }
    }
  }
}

pub struct LogInserter {
  fs_handle: std::thread::JoinHandle<std::io::Result<()>>,
  fs_tx: crossbeam_channel::Sender<FsBufMsg>,
  sinks: ahash::AHashMap<String, Arc<tokio::sync::Mutex<DatabaseSink>>>,
}

impl LogInserter {
  pub async fn new(
    db: db::Database,
    log_dir: PathBuf,
    buffer_lifetime: Duration,
    channels: &[crate::config::Channel],
  ) -> anyhow::Result<Self> {
    let mut db_sinks = ahash::AHashMap::with_capacity(channels.len());
    let mut fs_sinks = ahash::AHashMap::with_capacity(channels.len());
    for channel in channels {
      log::info!("Initializing the FS and DB sinks for {}", channel.name);
      let mut fs_sink = BackupLogSink::new(
        log_dir.clone(),
        channel.name.clone(),
        channel.message_buffer_size as usize * TWITCH_MESSAGE_SIZE,
      )?;
      let messages = fs_sink.read_existing_messages()?;
      let db_sink = DatabaseSink::new(
        db.clone(),
        channel.name.clone(),
        channel.username_cache_size,
        channel.message_buffer_size,
        messages,
      )
      .await?;
      db_sinks.insert(channel.name.clone(), Arc::new(tokio::sync::Mutex::new(db_sink)));
      fs_sinks.insert(channel.name.clone(), fs_sink);
    }

    let (fs_tx, fs_rx) = crossbeam_channel::unbounded();
    let fs_handle = FsMirrorBuffer::new(fs_rx, fs_sinks).spawn_writer_thread();
    Self::spawn_timed_autoflush_task(buffer_lifetime, fs_tx.clone(), db_sinks.values().cloned().collect());

    Ok(Self {
      fs_handle,
      fs_tx,
      sinks: db_sinks,
    })
  }

  pub fn join(self) -> anyhow::Result<()> {
    let _ = self.fs_tx.send(FsBufMsg::Stop);
    match self.fs_handle.join() {
      Ok(r) => r?,
      Err(e) => {
        anyhow::bail!("Failed to join the FS thread: {:?}", e);
      }
    }
    Ok(())
  }

  pub async fn insert_message(
    &self,
    message: db::logs::UnresolvedEntry,
  ) -> anyhow::Result<tokio::task::JoinHandle<anyhow::Result<bool>>> {
    let sink = self
      .sinks
      .get(message.channel())
      .cloned()
      .expect("Encountered an unregistered channel");

    let fs_tx = self.fs_tx.clone();
    fs_tx.try_send(FsBufMsg::Write(message.clone()))?;
    Ok(tokio::spawn(async move {
      DatabaseSink::insert_message(sink, fs_tx, message).await.map_err(|e| {
        log::error!("Failed to insert a message: {}\n{}", e, e.backtrace());
        e
      })
    }))
  }

  fn spawn_timed_autoflush_task(
    interval: std::time::Duration,
    tx: crossbeam_channel::Sender<FsBufMsg>,
    channels: Vec<Arc<tokio::sync::Mutex<DatabaseSink>>>,
  ) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
      loop {
        tokio::time::sleep(interval).await;

        log::info!("[AUTOFLUSH] === AUTOFLUSH STARTED ===");
        let timer = std::time::Instant::now();
        for channel in &channels {
          let mut sink = channel.lock().await;
          log::info!("[AUTOFLUSH] Flushing {}...", sink.channel);
          if sink.last_flushed.elapsed() > interval {
            match sink.flush(tx.clone()).await {
              Ok(_) => todo!(),
              Err(e) => {
                log::error!("Failed to flush a channel: {}", e);
              }
            }
          }
        }
        log::info!(
          "[AUTOFLUSH] === AUTOFLUSH ENDED ({}ms) ===",
          timer.elapsed().as_millis()
        );
      }
    })
  }
}

pub struct DatabaseSink {
  db: db::Database,
  channel: String,
  message_buf_size: usize,
  messages: db::logs::SOAEntry<i32>,
  usernames: db::UserCache,
  last_flushed: std::time::Instant,
}

impl DatabaseSink {
  pub async fn new(
    db: db::Database,
    channel: String,
    cache_size: usize,
    message_buf_size: usize,
    existing_messages: Vec<db::logs::UnresolvedEntry>,
  ) -> anyhow::Result<Self> {
    let mut me = Self {
      db,
      channel,
      messages: db::logs::SOAEntry::new(existing_messages.len().max(message_buf_size)),
      message_buf_size,
      usernames: db::UserCache::new(cache_size),
      last_flushed: std::time::Instant::now(),
    };

    for message in existing_messages {
      me.add_message(message).await?;
    }

    Ok(me)
  }

  async fn add_message(&mut self, message: db::logs::UnresolvedEntry) -> anyhow::Result<()> {
    let channel_id =
      db::channels::get_or_create_channel(&self.db, message.channel().as_ref(), true, &mut self.usernames).await?;
    let chatter_id =
      db::channels::get_or_create_channel(&self.db, message.chatter().as_ref(), false, &mut self.usernames).await?;
    self
      .messages
      .add(channel_id, chatter_id, message.sent_at, message.message);
    Ok(())
  }

  async fn insert_message(
    sink: Arc<tokio::sync::Mutex<Self>>,
    tx: crossbeam_channel::Sender<FsBufMsg>,
    message: db::logs::UnresolvedEntry,
  ) -> anyhow::Result<bool> {
    let mut sink = sink.lock().await;
    sink.add_message(message).await?;

    if sink.messages.size() >= sink.message_buf_size {
      sink.flush(tx).await?;
    }

    Ok(false)
  }

  async fn flush(&mut self, tx: crossbeam_channel::Sender<FsBufMsg>) -> anyhow::Result<()> {
    if self.messages.size() > 0 {
      log::info!("[{}] Inserting {} messages...", self.channel, self.messages.size());
      db::logs::insert_soa_resolved(&self.db, &mut self.messages).await?;
      tx.try_send(FsBufMsg::Rotate(self.channel.clone()))?;
      self.last_flushed = std::time::Instant::now();
    }
    Ok(())
  }
}

/// File sink which writes to a new file for each day
#[derive(Debug)]
pub struct BackupLogSink {
  channel: String,
  file: BufWriter<std::fs::File>,
}

fn open_log_file(dir: &Path, prefix: &str) -> std::io::Result<std::fs::File> {
  std::fs::OpenOptions::new()
    .write(true)
    .create(true)
    .append(true)
    .read(true)
    .open(dir.join(format!("{prefix}.log")))
}

impl BackupLogSink {
  pub fn new(mut log_dir: PathBuf, channel: String, buf_size: usize) -> std::io::Result<Self> {
    log_dir = log_dir.join(&channel);
    if !log_dir.exists() {
      fs::create_dir_all(&log_dir)?;
    }
    let file = open_log_file(&log_dir, &channel).map(|file| BufWriter::with_capacity(buf_size, file))?;
    Ok(BackupLogSink { channel, file })
  }

  pub fn write_message(&mut self, msg: db::logs::UnresolvedEntry) -> std::io::Result<()> {
    // Make sure that each individual message is always fully written to the disk
    let mut should_flush = false;

    let timestamp = msg.sent_at.to_rfc3339();
    let bytes = [
      // This should be faster than format!() allocating a comma-separated string because of the in-memory buffer
      &[b'\n'],
      msg.chatter.as_bytes(),
      &[b','],
      msg.message.as_bytes(),
      &[b','],
      timestamp.as_bytes(),
    ];

    for chunk in bytes {
      let chunk_size = chunk.len();
      let prev_buf_size = self.file.buffer().len();
      self.file.write_all(chunk)?;
      // the buffer was flushed, so we'll need to flush again to fully write the message to disk
      if self.file.buffer().len() < chunk_size + prev_buf_size {
        should_flush = true;
      }
    }

    if should_flush {
      self.file.flush()?;
    }

    Ok(())
  }

  pub fn read_existing_messages(&mut self) -> std::io::Result<Vec<db::logs::UnresolvedEntry>> {
    Ok(
      self
        .read_contents()?
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| -> Option<_> {
          let mut parts = line.splitn(2, ',');
          let chatter = parts.next()?;
          let message = parts.next()?;
          let timestamp = parts.next()?;
          let sent_at = DateTime::<chrono::Utc>::from(DateTime::parse_from_rfc3339(timestamp).ok()?);

          Some(db::logs::Entry::new(
            self.channel.clone(),
            chatter.to_owned(),
            sent_at,
            message.to_owned(),
          ))
        })
        .collect(),
    )
  }

  fn read_contents(&mut self) -> std::io::Result<String> {
    let mut out = String::with_capacity(self.file.get_ref().metadata()?.len() as usize);
    self.file.get_mut().read_to_string(&mut out)?;
    Ok(out)
  }

  pub fn rotate(&mut self) -> std::io::Result<()> {
    self.file.flush()?;
    self.file.get_mut().set_len(0)?;
    Ok(())
  }
}
