use serde::Deserialize;

pub mod chat;
pub mod collector;
pub mod train;

pub use self::{chat::ChatConfig, collector::CollectorConfig, train::TrainingConfig};

#[derive(Clone, Default, Debug, Deserialize)]
pub struct TwitchLogin {
  pub login: String,
  pub token: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GlobalConfig {
  #[serde(deserialize_with = "collector::CollectorConfig::deserialize")]
  pub collector: Option<collector::CollectorConfig>,
  pub chat: Option<chat::ChatConfig>,
  pub train: Option<train::TrainingConfig>,
}

impl GlobalConfig {
  pub fn load<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<GlobalConfig> {
    let content = std::fs::read_to_string(path)?;
    let mut config: GlobalConfig = serde_json::from_str::<GlobalConfig>(&content)?;

    config.collector = config.collector.map(|c| c.validate()).transpose()?;
    config.chat = config.chat.map(|c| c.validate()).transpose()?;
    config.train = config.train.map(|c| c.validate()).transpose()?;

    Ok(config)
  }
}
