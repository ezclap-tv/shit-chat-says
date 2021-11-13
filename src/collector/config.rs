use anyhow::Result;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_OUTPUT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "\\logs");
const DEFAULT_BUF_SIZE: usize = 1024; // 1 KiB

fn default_output_directory() -> std::path::PathBuf {
  std::path::PathBuf::from(DEFAULT_OUTPUT_DIRECTORY)
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
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct Config {
  pub channels: Vec<Channel>,
  pub output_directory: PathBuf,
}

impl From<TempConfig> for Config {
  fn from(c: TempConfig) -> Self {
    let TempConfig {
      channels,
      output_directory,
    } = c;
    Self {
      channels: channels.into_iter().map(Channel::from).collect(),
      output_directory,
    }
  }
}

impl Config {
  pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
    let content = fs::read_to_string(path)?;
    let config = Config::from(serde_json::from_str::<TempConfig>(&content)?);

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
}
