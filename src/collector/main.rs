pub mod config;
pub mod sink;

use anyhow::Result;
use config::Config;
use std::env;
use twitch::tmi::Message;

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

async fn run(db: db::Database, config: Config) -> Result<()> {
  let inserter = sink::LogInserter::new(
    db.clone(),
    config.filesystem_buffer_directory.clone(),
    config.buffer_lifetime,
    &config.channels,
  )
  .await?;

  'stop: loop {
    log::info!("Connecting to Twitch");
    let mut conn = twitch::tmi::connect(config.clone().into()).await.unwrap();
    for (i, channel) in config.channels.iter().enumerate() {
      log::info!("[{:<02} / {:<02}] Joining {}", i, config.channels.len(), channel.name);
      conn.sender.join(&channel.name).await?;
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
              inserter.insert_message(
                db::logs::UnresolvedEntry::new(channel.to_owned(), login.to_owned(), chrono::Utc::now(), text.to_owned())
              ).await?;
            },
            _ => ()
          },
          // recoverable error, reconnect
          Err(twitch::tmi::conn::Error::StreamClosed) => break,
          // parsing error, log and ignore
          Err(twitch::tmi::conn::Error::Parse(err)) => {
            log::error!("Failed to parse a message: {err}");
            continue;
          }
          // fatal error
          Err(err) => {
            log::error!("Fatal error: {}", err);
            break 'stop;
          }
        }
      }
    }
  }

  let _ = inserter.join();

  Ok(())
}

static CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[tokio::main]
async fn main() -> Result<()> {
  if env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "INFO");
  }
  scs_sentry::from_env!();
  env_logger::try_init()?;

  let url = env::var("SCS_DATABASE_URL").expect("SCS_DATABASE_URL must be set");

  let config = self::Config::load(&env::args().nth(1).map(std::path::PathBuf::from).unwrap_or_else(|| {
    std::path::PathBuf::from(CARGO_MANIFEST_DIR)
      .join("config")
      .join("collector.json")
  }))?;
  log::info!("{config:?}");

  let db = db::connect(url).await?;

  run(db, config).await
}
