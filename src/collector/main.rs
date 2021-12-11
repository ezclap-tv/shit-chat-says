pub mod config;
pub mod sink;

use anyhow::Result;
use config::Config;
use std::collections::HashMap;
use std::env;
use std::io::Write;
use twitch::Message;

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
    let mut conn = twitch::connect(config.clone().into()).await.unwrap();
    // one sink per channel
    let mut sinks = HashMap::<String, sink::DailyLogSink>::with_capacity(config.channels.len());
    for channel in config.channels.iter() {
      log::info!("Initializing sink for {}", channel.name);
      conn.sender.join(&channel.name).await?;
      sinks.insert(
        channel.name.clone(),
        sink::DailyLogSink::new(config.output_directory.clone(), channel.name.clone(), channel.buffer)?,
      );
    }
    log::info!("Logger is ready");

    loop {
      tokio::select! {
        _ = stop_signal() => {
          log::info!("Process terminated");
          break 'stop;
        },
        result = conn.reader.next() => match result {
          Ok(message) => match message {
            Message::Ping(ping) => conn.sender.pong(ping.arg()).await?,
            Message::Privmsg(message) => {
              let (channel, login, text) = (message.channel(), message.user.login(), message.text());
              log::info!("[{channel}] {login}: {text}");
              let mut sink = sinks.get_mut(channel).unwrap();
              write!(&mut sink, "{login},{text}\n")?;
            },
            _ => ()
          },
          // recoverable error, reconnect
          Err(twitch::conn::Error::StreamClosed) => break,
          // fatal error
          Err(err) => { log::error!("Fatal error: {}", err); break 'stop; }
        }
      }
    }

    for sink in sinks.values_mut() {
      sink.flush()?;
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

  let config = self::Config::load(&env::args().nth(1).map(std::path::PathBuf::from).unwrap_or_else(|| {
    std::path::PathBuf::from(CARGO_MANIFEST_DIR)
      .join("config")
      .join("collector.json")
  }))?;
  log::info!("{config:?}");

  run(config).await
}
