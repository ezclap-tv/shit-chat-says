use anyhow::Result;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_OUTPUT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "\\logs");
const DEFAULT_BUF_SIZE: usize = 25; // 25 messages, should be adjusted for faster chats.

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
  Buffered {
    name: String,
    message_buffer_size: usize,
  },
  BufferedWithCache {
    name: String,
    message_buffer_size: usize,
    username_cache_size: usize,
  },
}

#[derive(Deserialize)]
struct TempConfig {
  channels: Vec<TempChannel>,
  #[serde(deserialize_with = "humantime_serde::deserialize")]
  buffer_lifetime: std::time::Duration,
  #[serde(default = "default_output_directory")]
  filesystem_buffer_directory: PathBuf,
  credentials: Option<TwitchLogin>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TwitchLogin {
  pub login: String,
  pub token: String,
}

#[derive(Clone, Debug)]
pub struct Channel {
  pub name: String,
  pub message_buffer_size: usize,
  pub username_cache_size: usize,
}

impl From<TempChannel> for Channel {
  fn from(c: TempChannel) -> Self {
    match c {
      TempChannel::NameOnly(name) => Self {
        name,
        message_buffer_size: DEFAULT_BUF_SIZE,
        username_cache_size: crate::sink::USERNAME_CACHE_SIZE,
      },
      TempChannel::Buffered {
        name,
        message_buffer_size,
      } => Self {
        name,
        message_buffer_size,
        username_cache_size: crate::sink::USERNAME_CACHE_SIZE,
      },
      TempChannel::BufferedWithCache {
        name,
        message_buffer_size,
        username_cache_size,
      } => Self {
        name,
        message_buffer_size,
        username_cache_size,
      },
    }
  }
}

#[derive(Clone, Debug)]
pub struct Config {
  pub channels: Vec<Channel>,
  pub filesystem_buffer_directory: PathBuf,
  pub buffer_lifetime: std::time::Duration,
  pub credentials: Option<TwitchLogin>,
}

impl From<TempConfig> for Config {
  fn from(c: TempConfig) -> Self {
    let TempConfig {
      channels,
      filesystem_buffer_directory,
      buffer_lifetime,
      credentials,
    } = c;
    Self {
      channels: channels.into_iter().map(Channel::from).collect(),
      buffer_lifetime,
      filesystem_buffer_directory,
      credentials,
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

    if !config.filesystem_buffer_directory.exists() {
      log::warn!("config.filesystem_buffer_directory does not exist, it will be created.");
      std::fs::create_dir_all(&config.filesystem_buffer_directory)?;
    }

    if !config.filesystem_buffer_directory.is_dir() {
      log::error!("config.filesystem_buffer_directory is not a directory.");
      anyhow::bail!(format!(
        "{} is not a directory",
        config.filesystem_buffer_directory.display()
      ));
    }

    Ok(config)
  }
}

impl From<Config> for twitch::tmi::conn::Config {
  fn from(c: Config) -> Self {
    twitch::tmi::conn::Config {
      credentials: match c.credentials {
        Some(info) => twitch::tmi::conn::Login::Regular {
          login: info.login,
          token: info.token,
        },
        None => twitch::tmi::conn::Login::Anonymous,
      },
      membership_data: false,
    }
  }
}
