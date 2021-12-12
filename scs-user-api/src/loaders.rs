use std::collections::HashMap;

use cached::proc_macro::cached;
use juniper::futures::{AsyncReadExt, TryStreamExt};

use crate::schema;

#[inline]
fn bytes_to_megabytes(bytes: u64) -> f64 {
  (bytes as f64) / (1024.0 * 1024.0)
}

#[cached(size = 1, time = 3600, result = true, sync_writes = true)]
pub async fn load_channel_list(log_dir: std::path::PathBuf) -> anyhow::Result<HashMap<String, schema::Channel>> {
  log::info!(
    "Cache miss: reading the channel list from {log_dir}",
    log_dir = log_dir.display()
  );
  let mut channels = HashMap::with_capacity(32);

  let mut entries = async_fs::read_dir(log_dir).await?;
  while let Some(entry) = entries.try_next().await? {
    let metadata = entry.metadata().await?;
    if !metadata.is_dir() {
      continue;
    }

    let mut log_entries = async_fs::read_dir(entry.path()).await?;
    let mut log_files = Vec::with_capacity(356); // RAM is free
    let mut total_size = 0.0;

    while let Some(log_entry) = log_entries.try_next().await? {
      let metadata = log_entry.metadata().await?;

      if metadata.is_dir() {
        continue;
      }

      let size = bytes_to_megabytes(metadata.len());
      total_size += size;
      log_files.push(schema::LogFile {
        name: log_entry.file_name().to_string_lossy().into_owned(),
        size,
      });
    }

    let name = entry.file_name().to_string_lossy().into_owned();
    channels.insert(
      name.clone(),
      schema::Channel {
        name,
        log_files,
        total_size,
      },
    );
  }

  Ok(channels)
}

#[cached(size = 1, time = 3600, result = true, sync_writes = true)]
pub async fn load_model_list(model_dir: std::path::PathBuf) -> anyhow::Result<HashMap<String, schema::CachedModel>> {
  log::info!(
    "Cache miss: reading the model list from {model_dir}",
    model_dir = model_dir.display()
  );
  let mut models = HashMap::with_capacity(356);

  let mut entries = async_fs::read_dir(model_dir).await?;
  while let Some(entry) = entries.try_next().await? {
    let metadata = entry.metadata().await?;

    // TODO: list compressed model
    if metadata.is_dir() {
      continue;
    }

    let metadata = entry.metadata().await?;

    if metadata.is_dir() {
      continue;
    }

    let name = entry.file_name().to_string_lossy().into_owned();
    let size = bytes_to_megabytes(metadata.len());

    models.insert(
      name.clone(),
      schema::CachedModel {
        info: schema::ModelInfo {
          size,
          name,
          is_compressed: false,
          date_created: chrono::DateTime::from(metadata.created()?),
          date_modified: chrono::DateTime::from(metadata.modified()?),
        },
        path: entry.path().to_owned(),
        loaded: None,
      },
    );
  }

  Ok(models)
}

pub async fn load_model_list_and_refresh_model_meta_if_needed(
  context: &crate::SharedContext,
) -> anyhow::Result<HashMap<String, schema::CachedModel>> {
  let model_dir = context.read().await.model_dir.clone();
  let models = load_model_list(model_dir).await?;
  let existing_models = context.read().await.models.clone();

  println!("{:?}\n{:?}", existing_models.keys(), models.keys());

  // Locking per write is faster if the cache is warm
  for (name, model) in &models {
    if !existing_models.contains_key(&name[..])
      || model.info.date_modified > existing_models[&name[..]].info.date_modified
    {
      println!("{:?}", name);
      println!(
        "{} {:?}",
        model.info.date_modified.format("%F"),
        existing_models
          .get(&name[..])
          .map(|m| m.info.date_modified.format("%F"))
      );
      log::info!("Refreshing or adding `{}`", name);
      context.write().await.models.insert(name.clone(), model.clone());
    }
  }

  Ok(models)
}

pub(crate) async fn should_reload_model(
  path: &std::path::Path,
  last_modified: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<Option<schema::ModelInfo>> {
  let metadata = async_fs::metadata(path).await?;
  let fs_date_modified: chrono::DateTime<chrono::Utc> = metadata.modified()?.into();
  let info = if fs_date_modified > last_modified {
    Some(schema::ModelInfo {
      name: path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Missing the filename"))?
        .to_string_lossy()
        .into_owned(),
      size: bytes_to_megabytes(metadata.len()),
      is_compressed: false,
      date_created: chrono::DateTime::from(metadata.created()?),
      date_modified: chrono::DateTime::from(metadata.modified()?),
    })
  } else {
    None
  };
  Ok(info)
}

pub struct ThreadSafeGenerator(Box<dyn chain::TextGenerator>);
impl std::ops::Deref for ThreadSafeGenerator {
  type Target = dyn chain::TextGenerator;
  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

// This is OK because we don't actually use arbitrary types as TextGenerators,
// the trait is there to support markov chains of different orders.
unsafe impl Sync for ThreadSafeGenerator {}
#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for ThreadSafeGenerator {}

pub(crate) async fn load_model(path: &std::path::Path) -> anyhow::Result<(ThreadSafeGenerator, schema::ModelMeta)> {
  log::info!("Loading the model at `{path}`", path = path.display());
  let name = path
    .file_name()
    .ok_or_else(|| anyhow::anyhow!("Missing the filename"))?
    .to_string_lossy()
    .into_owned();
  let size = bytes_to_megabytes(async_fs::metadata(path).await?.len());

  let mut file = async_fs::File::open(path).await?;
  let mut buf = Vec::new();
  file.read_to_end(&mut buf).await?;

  let model = ThreadSafeGenerator(chain::load_chain_of_any_supported_order_with_reader(
    &mut std::io::Cursor::new(&buf),
  )?);
  let meta = schema::ModelMeta {
    name,
    size,
    order: model.order() as i32,
    metadata: model.model_meta_data().to_owned(),
  };
  log::info!("Successfully loaded the model at: {meta:?}`", meta = meta);

  Ok((model, meta))
}
