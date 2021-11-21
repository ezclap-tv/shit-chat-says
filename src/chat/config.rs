use anyhow::Result;
use serde::Deserialize;
use std::fs;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
  pub login: String,
  pub token: String,
  #[serde(default = "std::path::PathBuf::new")]
  pub model_path: std::path::PathBuf,
  pub channels: Vec<String>,
}

impl Config {
  pub fn load<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
    let config = serde_json::from_str::<Config>(
      &fs::read_to_string(path.as_ref()).map_err(|_| anyhow::anyhow!("Could not read 'chat.json' config file"))?,
    )?;
    if config.channels.is_empty() {
      anyhow::bail!("config.channels is empty, exiting.");
    }
    Ok(config)
  }
}

impl From<Config> for twitch::conn::Config {
  fn from(config: Config) -> Self {
    twitch::conn::Config {
      membership_data: false,
      credentials: twitch::conn::Login::Regular {
        login: config.login,
        token: config.token,
      },
    }
  }
}
