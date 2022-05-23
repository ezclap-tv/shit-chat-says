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

const MAX_LOCK_RETRIES: i32 = 25;
const LOCK_RETRY_DELAY: Duration = Duration::from_millis(300);

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

  fn spawn_writer_thread(
    self,
  ) -> (
    std::thread::JoinHandle<std::io::Result<()>>,
    Arc<std::sync::atomic::AtomicBool>,
  ) {
    use std::sync::atomic::Ordering;

    let me = Arc::new(tokio::sync::Mutex::new(self));
    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));

    let buf_ref = me.clone();
    let running_ref = running.clone();
    let handle = std::thread::spawn(move || {
      log::info!("[FS_MIRROR] Signal thread spawned");
      let mut signals = Signals::new(&[SIGHUP, SIGTERM, SIGINT, SIGQUIT])?;

      'outer: while running_ref.load(Ordering::SeqCst) {
        // We just want to consume a signal -- any signal -- and immediately terminate the wait loop.
        #[allow(clippy::never_loop)]
        for _ in signals.pending() {
          break 'outer;
        }
        std::thread::sleep(Duration::from_millis(100));
      }

      log::info!("[FS_MIRROR] Received a stop signal, shutting down...");
      running_ref.store(false, Ordering::SeqCst);

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

        if try_unwrap_count > MAX_LOCK_RETRIES {
          panic!("Couldn't obtain ownership of the fs buffer after 10 retries -- did the other thread exit after receiving the signal?")
        }

        log::info!("[FS_MIRROR] Attempt failed. Waiting for 100ms...");

        // Wait for 100ms for the other threads to abort()
        std::thread::sleep(LOCK_RETRY_DELAY);
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
            log::error!("Failed to flush one of the sinks: {}", e);
          }
        }
      }

      std::io::Result::Ok(())
    });

    let running_ref = running.clone();
    std::thread::spawn(move || {
      log::info!("[FS_MIRROR] Writer thread spawned");

      let mut me = me
        .try_lock()
        .expect("No other thread is holding the lock, so this can't fail");

      me.handle_write_messages(running_ref);
    });

    (handle, running)
  }

  fn handle_write_messages(&mut self, running: Arc<std::sync::atomic::AtomicBool>) {
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

    while running.load(std::sync::atomic::Ordering::SeqCst) {
      match self.rx.recv_timeout(Duration::from_millis(100)) {
        Ok(msg) => match msg {
          FsBufMsg::Write(entry) => {
            let sink = self.sinks.get_mut(entry.channel()).unwrap();
            on_err!(error_count, sink.write_message(entry));
          }
          FsBufMsg::Rotate(channel) => {
            let sink = self.sinks.get_mut(&channel).unwrap();
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
  fs_cond: Arc<std::sync::atomic::AtomicBool>,
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
        MessageBuf::from_entry_with_buf_size(channel.message_buffer_size, messages).await,
      );
      db_sinks.insert(channel.name.clone(), Arc::new(tokio::sync::Mutex::new(db_sink)));
      fs_sinks.insert(channel.name.clone(), fs_sink);
    }

    let (fs_tx, fs_rx) = crossbeam_channel::unbounded();
    let (fs_handle, fs_cond) = FsMirrorBuffer::new(fs_rx, fs_sinks).spawn_writer_thread();
    Self::spawn_timed_autoflush_task(buffer_lifetime, fs_tx.clone(), db_sinks.values().cloned().collect());

    Ok(Self {
      fs_handle,
      fs_tx,
      fs_cond,
      sinks: db_sinks,
    })
  }

  pub fn join(self) -> anyhow::Result<()> {
    let _ = self.fs_tx.send(FsBufMsg::Stop);
    self.fs_cond.store(false, std::sync::atomic::Ordering::SeqCst);
    match self.fs_handle.join() {
      Ok(r) => r?,
      Err(e) => {
        anyhow::bail!("Failed to join the FS thread: {:?}", e);
      }
    }
    Ok(())
  }

  pub async fn insert_message(&self, message: db::logs::UnresolvedEntry) -> anyhow::Result<bool> {
    let sink = self
      .sinks
      .get(message.channel())
      .cloned()
      .expect("Encountered an unregistered channel");

    let fs_tx = self.fs_tx.clone();
    fs_tx.try_send(FsBufMsg::Write(message.clone()))?;
    Ok(DatabaseSink::insert_message(sink, fs_tx, message).await)
  }

  fn spawn_timed_autoflush_task(
    interval: std::time::Duration,
    tx: crossbeam_channel::Sender<FsBufMsg>,
    channels: Vec<Arc<tokio::sync::Mutex<DatabaseSink>>>,
  ) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
      loop {
        log::info!("[AUTOFLUSH] === WAITING {}ms ===", interval.as_millis());
        tokio::time::sleep(interval).await;

        log::info!("[AUTOFLUSH] === AUTOFLUSH STARTED ===");
        let timer = std::time::Instant::now();
        for channel in &channels {
          let sink = channel.lock().await;
          log::info!("[AUTOFLUSH] Flushing {}...", sink.channel);

          if sink.last_flushed.elapsed() > interval {
            DatabaseSink::on_rotate(
              channel.clone(),
              sink,
              tx.clone(),
              move |sink_mux, channel, db, fs_tx, buf_tx, messages| async move {
                match DatabaseSink::flush(sink_mux, channel.clone(), db, fs_tx, buf_tx, messages).await {
                  Ok(n_messages) => {
                    log::info!(
                      "[AUTOFLUSH] [{}] === FINISHED FLUSHING ({}ms, {} messages) ===",
                      channel,
                      timer.elapsed().as_millis(),
                      n_messages,
                    );
                  }
                  Err(e) => {
                    log::error!("[AUTOFLUSH] [{}] Bulk insert failed: {}\n{}", channel, e, e.backtrace());
                  }
                }
              },
            )
            .await;
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

type BufRx = tokio::sync::mpsc::Receiver<db::logs::SOAEntry<String, String>>;
type BufTx = tokio::sync::mpsc::Sender<db::logs::SOAEntry<String, String>>;

pub struct MessageBuf {
  message_buf_size: usize,
  messages: db::logs::SOAEntry<String, String>,
  buf_rx: BufRx,
  buf_tx: BufTx,
}

impl MessageBuf {
  pub async fn new(message_buf_size: usize) -> Self {
    let (buf_tx, buf_rx) = tokio::sync::mpsc::channel(1);
    buf_tx
      .send(db::logs::SOAEntry::new(message_buf_size))
      .await
      .expect("Each channel is empty at initialization so this must succeed");
    Self {
      message_buf_size,
      messages: db::logs::SOAEntry::new(message_buf_size),
      buf_tx,
      buf_rx,
    }
  }

  pub async fn from_entry_with_buf_size(
    message_buf_size: usize,
    mut messages: db::logs::SOAEntry<String, String>,
  ) -> Self {
    let (buf_tx, buf_rx) = tokio::sync::mpsc::channel(1);
    buf_tx
      .send(db::logs::SOAEntry::new(message_buf_size))
      .await
      .expect("Each channel is empty at initialization so this must succeed");
    messages.reserve(message_buf_size);
    Self {
      buf_tx,
      buf_rx,
      message_buf_size,
      messages,
    }
  }

  #[inline]
  pub fn add_message(&mut self, message: db::logs::UnresolvedEntry) {
    self
      .messages
      .add(message.channel, message.chatter, message.sent_at, message.message);
  }

  #[inline]
  pub fn should_rotate(&self) -> bool {
    self.messages.size() >= self.message_buf_size
  }

  #[inline]
  pub async fn rotate(&mut self) -> (BufTx, db::logs::SOAEntry<String, String>) {
    let next_sink = self
      .buf_rx
      .recv()
      .await
      .expect("rx and tx are dropped at the same time, so this must succeed");
    let messages = std::mem::replace(&mut self.messages, next_sink);
    (self.buf_tx.clone(), messages)
  }
}

pub struct DatabaseSink {
  db: db::Database,
  channel: String,
  messages: MessageBuf,
  last_flushed: std::time::Instant,
}

impl DatabaseSink {
  pub fn new(db: db::Database, channel: String, messages: MessageBuf) -> Self {
    Self {
      db,
      channel,
      messages,
      last_flushed: std::time::Instant::now(),
    }
  }

  async fn insert_message(
    sink_mux: Arc<tokio::sync::Mutex<Self>>,
    fs_tx: crossbeam_channel::Sender<FsBufMsg>,
    message: db::logs::UnresolvedEntry,
  ) -> bool {
    let mut sink = sink_mux.lock().await;
    sink.messages.add_message(message);

    if sink.messages.should_rotate() {
      Self::on_rotate(
        sink_mux.clone(),
        sink,
        fs_tx,
        |sink_mux, channel, db, fs_tx, buf_tx, messages| async move {
          if let Err(e) = DatabaseSink::flush(sink_mux, channel, db, fs_tx, buf_tx, messages).await {
            log::error!("Bulk insert failed: {}\n{}", e, e.backtrace());
          }
        },
      )
      .await;
      true
    } else {
      false
    }
  }

  async fn on_rotate<F: futures::Future<Output = ()> + Send + 'static>(
    sink_mux: Arc<tokio::sync::Mutex<Self>>,
    mut sink: tokio::sync::MutexGuard<'_, Self>,
    fs_tx: crossbeam_channel::Sender<FsBufMsg>,
    future_factory: impl Fn(
      Arc<tokio::sync::Mutex<Self>>,
      String,
      db::Database,
      crossbeam_channel::Sender<FsBufMsg>,
      BufTx,
      db::logs::SOAEntry<String, String>,
    ) -> F,
  ) {
    // Copy the fields out of the sink to minimize the amount of time we hold the lock for when inserting the messages.
    let channel = sink.channel.clone();
    let db = sink.db.clone();
    let (buf_tx, messages) = sink.messages.rotate().await;
    std::mem::drop(sink);

    log::info!("[{}] Attempting to insert {} messages...", channel, messages.size());
    tokio::spawn(future_factory(sink_mux, channel, db, fs_tx, buf_tx, messages));
  }

  /// Inserts the messages in the buffer into the database and
  async fn flush(
    sink_mux: Arc<tokio::sync::Mutex<Self>>,
    channel: String,
    db: db::Database,
    fs_tx: crossbeam_channel::Sender<FsBufMsg>,
    buf_tx: BufTx,
    mut messages: db::logs::SOAEntry<String, String>,
  ) -> anyhow::Result<usize> {
    let size = messages.size();
    if size > 0 {
      db::logs::insert_soa_raw(&db, &mut messages).await?;
      // Rotate the fs sink first, so the previously buffered messages
      fs_tx.try_send(FsBufMsg::Rotate(channel))?;
      buf_tx.send(messages).await?;
      sink_mux.lock().await.last_flushed = std::time::Instant::now();
    } else {
      buf_tx.send(messages).await?; // we must return the buffer or the main thread will deadlock
    }
    Ok(size)
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

  pub fn read_existing_messages(&mut self) -> std::io::Result<db::logs::SOAEntry<String, String>> {
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

          Some((self.channel.clone(), chatter.to_owned(), sent_at, message.to_owned()))
        })
        .fold(
          db::logs::SOAEntry::new(512),
          |mut soa, (channel, chatter, sent_at, message)| {
            soa.add(channel, chatter, sent_at, message);
            soa
          },
        ),
    )
  }

  fn read_contents(&mut self) -> std::io::Result<String> {
    let mut out = String::with_capacity(self.file.get_ref().metadata()?.len() as usize);
    self.file.get_mut().read_to_string(&mut out)?;
    Ok(out)
  }

  pub fn rotate(&mut self) -> std::io::Result<()> {
    // clear the file first, then write any messages still buffered.
    self.file.get_mut().set_len(0)?;
    self.file.flush()?;
    Ok(())
  }
}
