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
  buf_size: usize,
  date: Date<Utc>,
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
      .open(log_dir.join(format!(
        "{prefix}-{date}.log",
        prefix = log_file_prefix,
        date = date.format("%F")
      )))
      .map(|file| BufWriter::with_capacity(buf_size, file))?;

    Ok(DailyLogSink {
      log_file_prefix,
      log_dir,
      buf_size,
      date,
      file,
    })
  }
}

impl Write for DailyLogSink {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    // ensure buffer never reaches `buf_size`
    if self.file.buffer().len() + buf.len() >= self.buf_size {
      self.file.flush()?;
    }
    // rotate file every day
    let today = Utc::today();
    if today.signed_duration_since(self.date).num_days() > 0 {
      self.date = today;
      self.file.flush()?;
      self.file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(self.log_dir.join(format!(
          "{prefix}-{date}.log",
          prefix = self.log_file_prefix,
          date = self.date.format("%F")
        )))
        .map(|file| BufWriter::with_capacity(self.buf_size, file))?;
    }
    // then actually write
    self.file.write(buf)
  }

  fn flush(&mut self) -> io::Result<()> {
    self.file.flush()
  }
}
