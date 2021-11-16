#![feature(format_args_capture)]

mod config;

use anyhow::Result;
use config::Config;
use std::env;
use twitch::Message;

type Model = markov::Chain<String>;

async fn run(config: Config) -> Result<()> {
  log::info!("Loading model");
  let model = Model::load(MODEL_DIR)?;

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
                model.generate_from_token(seed.to_string())
              } else {
                model.generate()
              };
              if !response.is_empty() {
                let response = response.join(" ");
                conn.sender.privmsg(channel, &response).await?;
              }
            }
          },
          _ => ()
        },
        Err(err) => log::error!("{}", err)
      }
    }
  }
}

const MODEL_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "\\data\\model.yaml");
const DEFAULT_CONFIG_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "\\chat.json");

#[tokio::main]
async fn main() -> Result<()> {
  if env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "INFO");
  }
  env_logger::try_init()?;

  let config = Config::load(&env::args().nth(1).unwrap_or_else(|| String::from(DEFAULT_CONFIG_PATH)))?;
  log::info!("{config:?}");

  run(config).await
}
