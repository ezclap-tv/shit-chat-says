use std::{
  collections::{HashMap, HashSet},
  path::PathBuf,
};

use chrono::{Date, NaiveDate, Utc};
use serde::Deserialize;

const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[derive(Debug, Clone, Deserialize)]
pub struct TrainingConfig {
  /// Internal time filter. Only set if `model_to_fine_tune` with a timestamped name is provided.
  #[serde(skip)]
  pub time_filter: Option<Date<Utc>>,
  /// A map of (channel name, data sources) to use for training. Defaults to all data if not provided.
  #[serde(default = "HashMap::<_, _>::default")]
  pub channels: HashMap<String, HashSet<String>>,
  /// The input directory containing properly formatted logs.
  #[serde(default = "default_input_directory")]
  pub input_directory: PathBuf,
  /// The output path or directory to save the model to.
  #[serde(default = "default_output_directory")]
  pub output_directory: PathBuf,
  /// Configures whether to save a timestamped copy of the model. Only works if `output_directory` is a directory.
  #[serde(default = "default_save_timestamped_checkpoint")]
  pub save_timestamped_checkpoint: bool,
  /// An optional path to a model that should be used for fine-tuning.
  pub model_to_fine_tune: Option<std::path::PathBuf>,
  /// If true, prefixes each sentence with the name of its author.
  #[serde(default = "default_authored_mode")]
  pub authored_mode: bool,
}

impl Default for TrainingConfig {
  fn default() -> Self {
    TrainingConfig {
      time_filter: None,
      channels: HashMap::new(),
      input_directory: default_input_directory(),
      output_directory: default_output_directory(),
      save_timestamped_checkpoint: default_save_timestamped_checkpoint(),
      model_to_fine_tune: None,
      authored_mode: false,
    }
  }
}

fn default_input_directory() -> PathBuf {
  std::env::var("SCS_INPUT_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|_| PathBuf::from(CARGO_MANIFEST_DIR).join("logs"))
}

fn default_output_directory() -> PathBuf {
  std::env::var("SCS_MODEL_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|_| PathBuf::from(CARGO_MANIFEST_DIR).join("models"))
}

fn default_save_timestamped_checkpoint() -> bool {
  true
}

fn default_authored_mode() -> bool {
  false
}

impl TrainingConfig {
  pub fn filter(&self, channel: &str, filename: &str) -> bool {
    filename.ends_with(".log")
      && if self.channels.is_empty() {
        true
      } else {
        self
          .channels
          .get(channel)
          .map_or(false, |channels| channels.contains(filename))
      }
  }

  pub fn is_after_date(&self, filename: &str) -> bool {
    self.time_filter.map_or(true, |min_date| {
      NaiveDate::parse_from_str(&filename[filename.len() - 14..], "%Y-%m-%d")
        .map(|file_date| file_date >= min_date.naive_utc())
        .map_err(|_| {
          log::error!("Log filename not timestamped, skipping: {}", filename);
        })
        .unwrap_or(false)
    })
  }

  pub fn load<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
    let content = std::fs::read_to_string(path)?;
    let mut config = serde_json::from_str::<Self>(&content)?;

    if config.channels.is_empty() {
      log::info!("config.channel is empty, the model will be trained on all logs.")
    }

    if !config.input_directory.exists() {
      log::error!("config.input_directory doesn't exist.");
      anyhow::bail!("Input directory doesn't exist")
    }

    if !config.output_directory.exists() {
      log::warn!("config.output_directory does not exist, it will be created.");
      std::fs::create_dir_all(&config.output_directory)?;
    }

    if !config.output_directory.is_dir() {
      log::error!("config.output_directory is not a directory");
      anyhow::bail!("config.output_directory must be a directory")
    }

    if let Some(model_path) = config.model_to_fine_tune.as_ref() {
      if !model_path.exists() {
        log::error!("config.model_to_fine_tune was provided, but doesn't exist on the disk");
        anyhow::bail!("config.model_to_fine_tune is invalid.")
      }
      if model_path.is_dir() {
        log::error!("config.model_to_fine_tune is a directory, expected a model file.");
        anyhow::bail!("config.model_to_fine_tune is invalid.")
      }

      // Attempt to extract the date the model was trained on from the filename.
      if let Some(date_str) = regex::Regex::new(r"\d{4}-\d{2}-\d{2}")
        .unwrap()
        .captures(&model_path.display().to_string())
        .and_then(|captures| captures.get(0))
        .map(|m| m.as_str())
      {
        config.time_filter = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
          .ok()
          .map(|d| Date::from_utc(d, Utc));
      }
    }

    log::info!("Loaded config: {:?}", config);

    Ok(config)
  }

  #[inline]
  pub fn extract_channel_name<'a>(&self, name: &'a str) -> &'a str {
    name.get(..name.len().wrapping_sub(15)).unwrap_or(name)
  }
}
