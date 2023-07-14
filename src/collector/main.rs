use std::env;
use std::time::Duration;

use anyhow::Result;
use tokio_tungstenite::tungstenite::Message;
use twitch::Command;

use config::Config;
use ingest::{fs::FileSystemSink, SinkManager};
use twitch_api::SuggestedAction;

pub mod config;

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
  let creds = twitch_api::Credentials::from(&config);
  let channel_names = config.channels.iter().map(|c| c.name.to_string()).collect::<Vec<_>>();
  let (mut manager, sender) =
    SinkManager::new(1024, Duration::from_secs(120)).expect("Failed to register stop signals");
  manager.add_sink(FileSystemSink::new(config.channels.clone(), &config.output_directory).await?);

  'stop: loop {
    log::info!("Connecting to Twitch");
    let mut conn = twitch_api::TwitchStream::new().await?;

    conn.authenticate(&creds).await?;
    conn.schedule_joins(&channel_names);

    log::info!("Entering main loop.");
    loop {
      let error = tokio::select! {
          _ = stop_signal() => {
            log::info!("Process terminated");
            manager.stop().await;
            break 'stop;
          },
          result = conn.receive() => match result {
            Ok(Some(message)) => if let Message::Text(batch) = message {
              handle_messages(&mut conn, &creds, &channel_names, &sender, batch).await
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
  }

  Ok(())
}

async fn handle_messages(
  conn: &mut twitch_api::TwitchStream,
  creds: &twitch_api::Credentials,
  channels: &[String],
  sender: &ingest::BatchSender,
  batch: String,
) -> std::result::Result<(), twitch_api::WsError> {
  let all_messages = batch
    .lines()
    .map(twitch::Message::parse)
    .filter_map(Result::ok)
    .collect::<Vec<_>>();

  // Process all the text messages first
  let mut batch = Vec::new();
  for twitch_msg in all_messages
    .iter()
    .filter(|msg| matches!(msg.command(), Command::Privmsg))
  {
    let channel = twitch_msg.channel().map(|c| c.strip_prefix('#').unwrap_or(c));
    let login = twitch_msg.prefix().and_then(|v| v.nick);
    let text = twitch_msg.text();

    if let (Some(channel), Some(login), Some(text)) = (channel, login, text) {
      log::info!("[{channel}] {login}: {text}");
      batch.push(ingest::sink::RawLogRecord::new(
        channel.into(),
        login.into(),
        chrono::Utc::now(),
        text.trim_end_matches(['\n', ' ', '\t', 'r', '\u{e0000}']).to_owned(),
      ));
    } else {
      log::warn!("Invalid message: {twitch_msg:?}");
    }
  }

  if !batch.is_empty() {
    sender.broadcast(batch);
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
