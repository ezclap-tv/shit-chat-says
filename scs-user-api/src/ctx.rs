use crate::schema;
use chrono::DateTime;
use futures::TryStreamExt;
use std::{ffi::OsStr, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

#[inline]
fn bytes_to_megabytes(bytes: u64) -> f64 {
  (bytes as f64) / (1024.0 * 1024.0)
}

pub struct State {
  models_dir: PathBuf,
}

impl State {
  pub fn new(models_dir: PathBuf) -> Self {
    Self { models_dir }
  }

  /// Returns a list of models
  pub async fn get_models(&self) -> anyhow::Result<Vec<schema::SimpleModelInfo>> {
    // TODO: load the model to acquire `order` and `channels`
    // after loading, put it in a cache which:
    //   - evicts after some time
    //   - reloads if a new version is available
    use anyhow::Context;

    let mut models = Vec::new();

    let mut entries = async_fs::read_dir(&self.models_dir).await?;
    while let Some(entry) = entries.try_next().await? {
      let metadata = entry.metadata().await?;
      let path = entry.path();

      if path.extension() != Some(OsStr::new("chain")) {
        continue;
      }

      let name = path
        .file_stem()
        .map(|v| v.to_string_lossy())
        .context("Invalid file stem")?
        .to_string();
      let date_created = DateTime::from(metadata.created()?);
      let date_modified = DateTime::from(metadata.modified()?);
      let size = bytes_to_megabytes(metadata.len());

      models.push(schema::SimpleModelInfo {
        name,
        date_created,
        date_modified,
        size,
      })
    }

    Ok(models)
  }
}

#[derive(Clone)]
pub struct Context(Arc<RwLock<State>>);

impl Context {
  pub fn new(state: State) -> Self {
    Self(Arc::new(RwLock::new(state)))
  }

  pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, State> {
    self.0.read().await
  }

  pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, State> {
    self.0.write().await
  }
}
