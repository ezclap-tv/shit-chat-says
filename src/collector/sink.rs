use chrono::{Date, Utc};
use std::{
  fs,
  io::{self, BufWriter, Write},
  path::PathBuf,
};

/// File sink which writes to a new file for each day
pub struct DailyLogSink {
  log_file_prefix: String,
  log_dir: PathBuf,
  bytes_written: usize,
  buf_size: usize,
  current_date: Date<Utc>,
  file: BufWriter<std::fs::File>,
}

impl DailyLogSink {
  pub fn new(log_dir: PathBuf, log_file_prefix: String, buf_size: usize) -> io::Result<Self> {
    if !log_dir.exists() {
      fs::create_dir_all(&log_dir)?;
    }
    let date = Utc::today();
    let file = fs::OpenOptions::new()
      .create(true)
      .append(true)
      .open(log_dir.join(format!("{log_file_prefix}-{date}.log")))
      .map(|file| BufWriter::with_capacity(buf_size, file))?;

    Ok(DailyLogSink {
      log_file_prefix,
      log_dir,
      bytes_written: 0,
      buf_size,
      current_date: date,
      file,
    })
  }
}

impl Write for DailyLogSink {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    // ensure buffer never reaches `buf_size`
    if self.bytes_written + buf.len() >= self.buf_size {
      self.file.flush()?;
    }
    // rotate file every day
    let date = Utc::today();
    if date.signed_duration_since(self.current_date).num_days() > 0 {
      self.file.flush()?;
      self.file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(self.log_dir.join(format!("{}-{}.log", self.log_file_prefix, date)))
        .map(|file| BufWriter::with_capacity(self.buf_size, file))?;
    }
    // actually write
    self.bytes_written += buf.len();
    self.file.write(buf)
  }

  fn flush(&mut self) -> io::Result<()> {
    self.file.flush()
  }
}
