#![feature(format_args_capture)]

mod config;

use anyhow::Result;
use config::Config;
use std::{env, path::PathBuf};
use twitch::Message;

// Set to 0 to disable sampling.
const MAX_SAMPLES: usize = 16;

async fn run(config: Config) -> Result<()> {
  log::info!("Loading model");
  let model = chain::load_chain_of_any_supported_order(&config.model_path)?;

  log::info!("Connecting to Twitch");
  let mut conn = twitch::connect(config.clone().into()).await.unwrap();
  // one sink per channel
  for channel in &config.channels {
    log::info!("Joining channel '{}'", channel);
    conn.sender.join(channel).await?;
  }
  let prefix = format!("@{}", config.login.to_ascii_lowercase());
  log::info!("Chat bot is ready");

  loop {
    tokio::select! {
      _ = tokio::signal::ctrl_c() => {
        log::info!("CTRL-C");
        break Ok(())
      },
      result = conn.reader.next() => match result {
        Ok(message) => match message {
          Message::Ping(ping) => conn.sender.pong(ping.arg()).await?,
          Message::Privmsg(message) => {
            let (channel, login, text) = (message.channel(), message.user.login(), message.text());
            log::info!("[{channel}] {login}: {text}");
            // format: `@LOGIN <seed> <...rest>`
            // `rest` is ignored
            if text.to_ascii_lowercase().starts_with(&prefix) {
              let response = if let Some(seed) = text.split_whitespace().nth(1) {
                chain::sample(&model, seed, MAX_SAMPLES)
              } else {
                chain::sample(&model, "", MAX_SAMPLES)
              };
              if !response.is_empty() {
                conn.sender.privmsg(channel, &response).await?;
              }
            }
          },
          _ => ()
        },
        Err(err) => {
          log::error!("{err}");
          break Ok(())
        }
      }
    }
  }
}

const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[tokio::main]
async fn main() -> Result<()> {
  if env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "INFO");
  }
  env_logger::try_init()?;

  let mut config = Config::load(
    &env::args()
      .nth(1)
      .map(PathBuf::from)
      .unwrap_or_else(|| PathBuf::from(CARGO_MANIFEST_DIR).join("config").join("chat.json")),
  )?;

  if config.model_path.as_os_str().is_empty() {
    config.model_path = std::env::var("SCS_MODEL_PATH")
      .map(PathBuf::from)
      .unwrap_or_else(|_| PathBuf::from(CARGO_MANIFEST_DIR).join("models").join("model.chain"));
  }

  log::info!("{config:?}");

  run(config).await
}
