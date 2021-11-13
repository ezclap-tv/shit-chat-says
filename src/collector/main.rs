#![feature(format_args_capture)]

pub mod config;
pub mod sink;

use anyhow::Result;
use config::Config;
use std::collections::HashMap;
use std::env;
use std::io::Write;
use twitch::Message;

// TODO: handle TMI restarts + disconnections with retry

async fn run(config: Config) -> Result<()> {
  log::info!("Connecting to Twitch");
  let mut conn = twitch::connect(twitch::Config::default()).await.unwrap();
  // one sink per channel
  let mut sinks = HashMap::<String, sink::DailyLogSink>::with_capacity(config.channels.len());
  for channel in config.channels.into_iter() {
    log::info!("Initializing sink for {}", channel.name);
    conn.sender.join(&channel.name).await?;
    sinks.insert(
      channel.name.clone(),
      sink::DailyLogSink::new(config.output_directory.clone(), channel.name, channel.buffer)?,
    );
  }
  log::info!("Logger is ready");

  loop {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            log::info!("CTRL-C");
            break;
        },
        result = conn.reader.next() => match result {
            Ok(message) => match message {
                Message::Ping(ping) => conn.sender.pong(ping.arg()).await?,
                Message::Privmsg(message) => {
                  let (channel, login, text) = (message.channel(), message.user.login(), message.text());
                  let time = chrono::Utc::now().format("%T");
                  log::info!("[{channel}] {login}: {text}");
                  let mut sink = sinks.get_mut(channel).unwrap();
                  write!(&mut sink, "{channel},{time},{login},{text}\n")?;
                },
                _ => ()
            },
            Err(err) => log::error!("{}", err)
        }
    }
  }

  for sink in sinks.values_mut() {
    sink.flush()?;
  }

  Ok(())
}

const DEFAULT_CONFIG_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "\\collector.json");

#[tokio::main]
async fn main() -> Result<()> {
  if std::env::var("RUST_LOG").is_err() {
    std::env::set_var("RUST_LOG", "INFO");
  }
  env_logger::try_init()?;

  let config = self::Config::load(&env::args().nth(1).unwrap_or_else(|| String::from(DEFAULT_CONFIG_PATH)))?;
  log::info!("{config:?}");

  run(config).await
}
