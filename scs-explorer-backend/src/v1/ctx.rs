use actix_web::Result;
use futures::{io::BufReader, AsyncBufReadExt, TryStreamExt};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

pub const PAGE_SIZE: usize = 100;

pub struct State {
  pub logs_dir: PathBuf,
  pub models_dir: PathBuf,
}

impl State {
  pub fn new(logs_dir: PathBuf, models_dir: PathBuf) -> Self {
    Self { logs_dir, models_dir }
  }

  /// Returns a list of channel names for which logs are available
  pub async fn get_logged_channels(&self) -> Result<Vec<String>> {
    let mut stream = async_fs::read_dir(&self.logs_dir).await?;
    let mut out = Vec::new();
    while let Some(entry) = stream.try_next().await? {
      out.push(entry.path().file_name().unwrap().to_string_lossy().to_string())
    }
    Ok(out)
  }

  /// Returns messages for `channel`, as a String with messages separated by newlines,
  /// and a page token which can be used to resume reading.
  pub async fn get_logs<S: AsRef<str>>(
    &self,
    channel: S,
    page_token: Option<&str>,
  ) -> Result<Option<(String, String)>> {
    // TODO: ingest logs into a database and use that instead?

    let channel = channel.as_ref();
    let page_token_file = match page_token {
      Some(token) => base64::decode(token)
        .ok()
        .map(|v| String::from_utf8(v).ok())
        .flatten()
        .map(std::ffi::OsString::from),
      None => None,
    };

    let mut messages = String::new();
    let mut lines = 0usize;
    let mut found_last_read_file = page_token_file.is_none();
    let mut current_file = PathBuf::new();

    let mut stream = async_fs::read_dir(&self.logs_dir.join(channel)).await?;
    while let Some(entry) = stream.try_next().await? {
      let path = entry.path();
      if found_last_read_file {
        current_file = path;
        let mut file = BufReader::new(async_fs::OpenOptions::new().read(true).open(&current_file).await?).lines();
        while let Some(line) = file.try_next().await? {
          if !line.is_empty() {
            messages.push_str(&line);
            messages.push('\n');
            lines += 1;
          }
        }

        // stop reading once we've reached enough lines
        if lines >= PAGE_SIZE {
          return Ok(Some((
            messages,
            base64::encode(current_file.file_name().unwrap().to_string_lossy().as_bytes()),
          )));
        }
      } else if path.file_name() == page_token_file.as_deref() {
        // skip files until we find the last one we read
        // this works because the files are read in order by date (hopefully?)
        found_last_read_file = true;
        // we already read this file, so we don't have to read it again
      }
    }

    if found_last_read_file {
      // we read till the end but didn't get more than `PAGE_SIZE` lines
      Ok(Some((
        messages,
        base64::encode(current_file.file_name().unwrap().to_string_lossy().as_bytes()),
      )))
    } else {
      Ok(None)
    }
  }

  /// Returns a list of available models
  pub async fn get_models(&self) -> Result<Vec<String>> {
    let mut stream = async_fs::read_dir(&self.models_dir).await?;
    let mut out = Vec::new();
    while let Some(entry) = stream.try_next().await? {
      out.push(entry.path().file_name().unwrap().to_string_lossy().to_string())
    }
    Ok(out)
  }
}

impl Default for State {
  fn default() -> Self {
    Self {
      logs_dir: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("logs"),
      models_dir: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("models"),
    }
  }
}

#[derive(Clone, Default)]
pub struct Context(Arc<RwLock<State>>);

impl Context {
  pub fn new(ctx: State) -> Self {
    Self(Arc::new(RwLock::new(ctx)))
  }

  pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, State> {
    self.0.read().await
  }

  pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, State> {
    self.0.write().await
  }
}
