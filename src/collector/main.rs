use std::collections::HashMap;
use std::env;
use std::io::Write;

use anyhow::Result;
use tokio_tungstenite::tungstenite::Message;
use twitch::Command;

use config::Config;
use twitch_api::SuggestedAction;

pub mod config;
pub mod sink;

use sink::DailyLogSink;
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

    conn.authenticate(&creds).await?;
    conn.schedule_joins(&channel_names);

    log::info!("Entering main loop.");
    loop {
      let error = tokio::select! {
          _ = stop_signal() => {
            log::info!("Process terminated");
            for sink in sinks.values_mut() {
              sink.flush()?;
            }
            break 'stop;
          },
          result = conn.receive() => match result {
            Ok(Some(message)) => if let Message::Text(batch) = message {
              handle_messages(&mut conn, &creds, &channel_names, &mut sinks, batch).await
            } else {
              Ok(())
            },
            Ok(None) => break,
            Err(e) => Err(e),
          },
      };

      if let Err(e) = error {
        log::error!("Error receiving or processing messages: {:?}", e);
        let action = SuggestedAction::from(&e);
        match action {
          SuggestedAction::KeepGoing => (),
          SuggestedAction::Reconnect => break,
          SuggestedAction::Terminate => break 'stop,
        }
      }
    }

    for sink in sinks.values_mut() {
      sink.flush()?;
    }
  }

  Ok(())
}

async fn handle_messages(
  conn: &mut twitch_api::TwitchStream,
  creds: &twitch_api::Credentials,
  channels: &[String],
  sinks: &mut HashMap<String, DailyLogSink>,
  batch: String,
) -> std::result::Result<(), twitch_api::WsError> {
  let all_messages = batch
    .lines()
    .map(twitch::Message::parse)
    .filter_map(Result::ok)
    .collect::<Vec<_>>();

  // Process all the text messages first
  for twitch_msg in all_messages
    .iter()
    .filter(|msg| matches!(msg.command(), Command::Privmsg))
  {
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
  }

  for twitch_msg in all_messages
    .into_iter()
    .filter(|msg| !matches!(msg.command(), Command::Privmsg))
  {
    match twitch_msg.command() {
      Command::Ping => conn.pong().await?,
      Command::Reconnect => conn.reconnect(creds, channels).await?,
      _ => (),
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
