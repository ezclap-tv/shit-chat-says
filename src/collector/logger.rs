use std::{
  collections::HashMap,
  io::{BufWriter, Write},
};

pub const BUF_SIZE_TO_FLUSH: usize = 1024; // 1KB

pub struct ChatSink {
  bytes_written: usize,
  file: BufWriter<std::fs::File>,
}

pub struct ChatLogger {
  queue: crossbeam_channel::Receiver<twitch::Privmsg>,
  // No need to manually close the files, they are closed automatically on drop
  current_date: chrono::Date<chrono::Utc>,
  output_directory: std::path::PathBuf,
  handles: HashMap<String, ChatSink>,
}

impl ChatLogger {
  pub fn new(log_directory: std::path::PathBuf, queue: crossbeam_channel::Receiver<twitch::Privmsg>) -> Self {
    Self {
      queue,
      output_directory: log_directory,
      current_date: chrono::Utc::today(),
      handles: HashMap::new(),
    }
  }

  pub fn add_channels(&mut self, channels: Vec<String>) -> Result<(), anyhow::Error> {
    for channel in channels {
      self.rotate_log_file(&channel)?;
    }
    Ok(())
  }

  pub fn spawn_thread(mut self) -> std::thread::JoinHandle<Result<(), anyhow::Error>> {
    std::thread::spawn(move || {
      while let Ok(msg) = self.queue.recv() {
        let now = chrono::Utc::today();
        let date_has_changed = now.signed_duration_since(self.current_date).num_days() > 0;

        let sink = match self.handles.get_mut(msg.channel()) {
          None => self.rotate_log_file(msg.channel())?,
          Some(_) if date_has_changed => self.rotate_log_file(msg.channel())?,
          Some(handle) => handle,
        };

        Self::write(sink, msg.user.name.as_bytes())?;
        Self::write(sink, b": ")?;
        Self::write(sink, msg.text().as_bytes())?;
        Self::write(sink, b"\n")?;

        #[cfg(debug_assertions)]
        log::info!("Logging a message in {} | buf={}", msg.channel(), sink.bytes_written);

        if sink.bytes_written > BUF_SIZE_TO_FLUSH {
          log::info!("Flushing {}b into the file in {}", sink.bytes_written, msg.channel());

          sink.file.flush()?;
          sink.bytes_written = 0;
        }
      }

      log::info!("Failed to recv from the queue, stopping the logger thread.");
      Ok(())
    })
  }

  fn rotate_log_file(&mut self, channel: &str) -> Result<&mut ChatSink, anyhow::Error> {
    let directory = self.output_directory.join(channel);
    let path = directory.join(format!("{channel}-{}.log", self.current_date));

    if !directory.exists() {
      std::fs::create_dir_all(&directory)?;
    }

    let sink = if self.handles.contains_key(channel) {
      // This is to convince the borrow checker
      let sink = self.handles.get_mut(channel).unwrap();
      *sink = Self::open(&path)?;
      sink
    } else {
      self.handles.insert(channel.to_owned(), Self::open(&path)?);
      self.handles.get_mut(channel).unwrap()
    };

    log::info!("Writing to {}", path.display());

    Ok(sink)
  }

  fn open(path: &std::path::Path) -> Result<ChatSink, std::io::Error> {
    std::fs::OpenOptions::new()
      .create(true)
      .append(true)
      .open(path)
      .map(|file| ChatSink {
        bytes_written: 0,
        file: BufWriter::new(file),
      })
  }

  #[inline]
  fn write(sink: &mut ChatSink, bytes: &[u8]) -> Result<(), std::io::Error> {
    sink.bytes_written += bytes.len();
    sink.file.write_all(bytes)
  }
}
