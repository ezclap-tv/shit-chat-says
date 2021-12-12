use serde::Deserialize;
use std::time::Duration;

#[derive(Clone, Debug, Deserialize)]
pub struct ChatConfig {
  pub credentials: crate::TwitchLogin,
  #[serde(default = "std::path::PathBuf::new")]
  pub model_path: std::path::PathBuf,
  pub channels: Vec<String>,
  #[serde(default = "default_reply_probability")]
  pub reply_probability: f64,
  #[serde(with = "humantime_serde")]
  #[serde(default = "default_reply_timeout")]
  pub reply_timeout: Duration,
  #[serde(default = "default_message_count")]
  pub reply_after_messages: usize,
  #[serde(default = "std::collections::HashSet::new")]
  pub reply_blocklist: std::collections::HashSet<String>,
}

impl Default for ChatConfig {
  fn default() -> Self {
    Self {
      credentials: crate::TwitchLogin::default(),
      model_path: std::path::PathBuf::new(),
      channels: vec![],
      reply_probability: default_reply_probability(),
      reply_timeout: default_reply_timeout(),
      reply_after_messages: default_message_count(),
      reply_blocklist: std::collections::HashSet::new(),
    }
  }
}

const fn default_reply_probability() -> f64 {
  0.0
}

const fn default_reply_timeout() -> Duration {
  Duration::from_secs(60)
}

const fn default_message_count() -> usize {
  10
}

impl ChatConfig {
  pub fn validate(self) -> anyhow::Result<Self> {
    let mut config = self;
    config.reply_blocklist = config
      .reply_blocklist
      .into_iter()
      .map(|s| s.to_ascii_lowercase())
      .collect();
    Ok(config)
  }
}
