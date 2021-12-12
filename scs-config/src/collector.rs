use std::path::PathBuf;

use serde::Deserialize;

const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");
const DEFAULT_BUF_SIZE: usize = 1024; // 1 KiB

fn default_output_directory() -> std::path::PathBuf {
  std::path::PathBuf::from(CARGO_MANIFEST_DIR).join("logs")
}

// We only want `Buffered`, but the user should be able to write
// just the name, without having to specify the buffer size.
// We also don't want this distinction when using the channel list,
// each channel should have an associated buffer size, so we split
// the deserialization into two steps.

#[derive(Deserialize)]
#[serde(untagged)]
pub enum TempChannel {
  NameOnly(String),
  Buffered { name: String, buffer: usize },
}

#[derive(Deserialize)]
struct TempConfig {
  channels: Vec<TempChannel>,
  #[serde(default = "default_output_directory")]
  output_directory: PathBuf,
  credentials: Option<crate::TwitchLogin>,
}

#[derive(Clone, Debug)]
pub struct Channel {
  pub name: String,
  pub buffer: usize,
}

impl From<TempChannel> for Channel {
  fn from(c: TempChannel) -> Self {
    match c {
      TempChannel::NameOnly(name) => Self {
        name,
        buffer: DEFAULT_BUF_SIZE,
      },
      TempChannel::Buffered { name, buffer } => Self { name, buffer },
    }
  }
}

#[derive(Clone, Debug)]
pub struct CollectorConfig {
  pub channels: Vec<Channel>,
  pub output_directory: PathBuf,
  pub credentials: Option<crate::TwitchLogin>,
}

impl From<TempConfig> for CollectorConfig {
  fn from(c: TempConfig) -> Self {
    let TempConfig {
      channels,
      output_directory,
      credentials,
    } = c;
    Self {
      channels: channels.into_iter().map(Channel::from).collect(),
      output_directory,
      credentials,
    }
  }
}

impl CollectorConfig {
  pub fn validate(self) -> anyhow::Result<Self> {
    let config = self;

    if config.channels.is_empty() {
      log::error!("config.channels is empty, exiting.");
      anyhow::bail!("No channels specified");
    }

    if !config.output_directory.exists() {
      log::warn!("config.output_directory does not exist, it will be created.");
      std::fs::create_dir_all(&config.output_directory)?;
    }

    if !config.output_directory.is_dir() {
      log::error!("config.output_directory is not a directory.");
      anyhow::bail!(format!("{} is not a directory", config.output_directory.display()));
    }

    Ok(config)
  }

  pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<Option<CollectorConfig>, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    let json: Option<TempConfig> = serde::de::Deserialize::deserialize(deserializer)?;
    Ok(json.map(CollectorConfig::from))
  }
}
