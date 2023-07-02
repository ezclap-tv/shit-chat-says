pub mod config;
pub mod sink;

use anyhow::Result;
use config::Config;
use std::collections::HashMap;
use std::env;
use std::io::Write;
use twitch::Command;

// TODO: handle TMI restarts + disconnections with retry

#[cfg(target_family = "windows")]
use tokio::signal::ctrl_c as stop_signal;

#[cfg(target_family = "unix")]
async fn stop_signal() -> std::io::Result<()> {
  let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?; // SIGTERM for docker-compose down
  let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?; // SIGINT for ctrl-c

  let sigterm = sigterm.recv();
  let sigint = sigint.recv();

  tokio::select! {
    _ = sigterm => Ok(()),
    _ = sigint => Ok(()),
  }
}

async fn run(config: Config) -> Result<()> {
  'stop: loop {
    log::info!("Connecting to Twitch");
    let mut conn = twitch_api::TwitchStream::new().await?;
    let creds = twitch_api::Credentials::from(&config);
    let channel_names = config.channels.iter().map(|c| c.name.clone()).collect::<Vec<_>>();

    // one sink per channel
    let mut sinks = HashMap::<String, sink::DailyLogSink>::with_capacity(config.channels.len());
    for channel in config.channels.iter() {
      log::info!("Initializing sink for {}", channel.name);
      sinks.insert(
        channel.name.clone(),
        sink::DailyLogSink::new(config.output_directory.clone(), channel.name.clone(), channel.buffer)?,
      );
    }
    conn.init(&creds, &channel_names).await?;
    log::info!("Logger is ready");

    let mut error_count = 0;

    loop {
      tokio::select! {
        _ = stop_signal() => {
          log::info!("Process terminated");
          for sink in sinks.values_mut() {
            sink.flush()?;
          }
          break 'stop;
        },
        result = conn.receive() => match result {
          Ok(Some(batch)) => for twitch_msg in batch.lines().map(twitch::Message::parse).filter_map(Result::ok) {
            match twitch_msg.command() {
              Command::Ping => conn.pong().await?,
              Command::Reconnect => conn.reconnect(&creds, &channel_names).await?,
              Command::Privmsg => {
                let channel = twitch_msg.channel().map(|c| c.strip_prefix('#').unwrap_or(c));
                let login = twitch_msg.prefix().and_then(|v| v.nick);
                let text = twitch_msg.text();

                if let (Some(channel), Some(login), Some(text)) = (channel, login, text) {
                  log::info!("[{channel}] {login}: {text}");
                  let mut sink = sinks.get_mut(channel).unwrap();
                  write!(&mut sink, "{login},{text}\n")?;
                } else {
                  log::warn!("Invalid message: {twitch_msg:?}");
                }
              },
              _ => ()
            }
          }
          Ok(_) => (),
          Err(e) => {
            log::error!("Error receiving messages: {}", e);
            error_count += 1;
            if error_count > 5 {
              log::error!("Too many receive errors, reconnecting");
              break;
            }
          }
        }
      }
    }
  }

  Ok(())
}

static CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[tokio::main]
async fn main() -> Result<()> {
  if env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "INFO");
  }
  env_logger::try_init()?;

  let config = self::Config::load(env::args().nth(1).map(std::path::PathBuf::from).unwrap_or_else(|| {
    std::path::PathBuf::from(CARGO_MANIFEST_DIR)
      .join("config")
      .join("collector.json")
  }))?;
  log::info!("{config:?}");

  run(config).await
}
