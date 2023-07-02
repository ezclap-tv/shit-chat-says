use chrono::{DateTime, Utc};
use std::{
  fs::{self, File},
  io::{self, BufWriter, Write},
  path::{Path, PathBuf},
};

/// File sink which writes to a new file for each day
pub struct DailyLogSink {
  log_file_prefix: String,
  log_dir: PathBuf,
  date: DateTime<Utc>,
  file: BufWriter<std::fs::File>,
}

fn open_log_file(dir: &Path, prefix: &str) -> io::Result<File> {
  let date = Utc::now().format("%F");
  fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(dir.join(format!("{prefix}-{date}.log")))
}

impl DailyLogSink {
  pub fn new(mut log_dir: PathBuf, log_file_prefix: String, buf_size: usize) -> io::Result<Self> {
    log_dir = log_dir.join(&log_file_prefix);
    if !log_dir.exists() {
      fs::create_dir_all(&log_dir)?;
    }
    let date = Utc::now();
    let file = open_log_file(&log_dir, &log_file_prefix).map(|file| BufWriter::with_capacity(buf_size, file))?;

    Ok(DailyLogSink {
      log_file_prefix,
      log_dir,
      date,
      file,
    })
  }
}

impl Write for DailyLogSink {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    // rotate file every day
    let today = Utc::now();
    if today.signed_duration_since(self.date).num_days() > 0 {
      self.date = today;
      self.file.flush()?;
      *self.file.get_mut() = open_log_file(&self.log_dir, &self.log_file_prefix)?;
    }
    // then actually write
    self.file.write(buf)
  }

  fn flush(&mut self) -> io::Result<()> {
    self.file.flush()
  }
}
