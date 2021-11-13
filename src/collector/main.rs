#![feature(format_args_capture)]
use twitch::{Config, Message};

pub mod logger;

pub const DEFAULT_CONFIG_PATH: &str = "./collector.json";
pub const DEFAULT_OUTPUT_DIRECTORY: &str = "./logs";

pub fn default_output_directory() -> std::path::PathBuf {
  std::path::PathBuf::from(DEFAULT_OUTPUT_DIRECTORY)
}

#[derive(Debug, serde::Deserialize)]
pub struct CollectorConfig {
  channels: Vec<String>,
  #[serde(default = "default_output_directory")]
  output_directory: std::path::PathBuf,
}

async fn run(config: CollectorConfig) -> Result<(), anyhow::Error> {
  // QQQ: ingest from multiple threads?
  // QQQ: do we want multiple writers as well? If we do, do we want to lock on write or split the channels between N internally-synchronized threads?
  let mut conn = twitch::connect(Config::default()).await.unwrap();

  for channel in &config.channels {
    log::info!("Joining {channel}");
    conn.sender.join(channel).await?;
  }

  let (tx, rx) = crossbeam_channel::unbounded();
  let mut logger = logger::ChatLogger::new(config.output_directory, rx);
  logger.add_channels(config.channels)?;
  let _handle = logger.spawn_thread();

  loop {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            log::info!("CTRL-C");
            break;
        },
        result = conn.reader.next() => match result {
            Ok(message) => match message {
                Message::Ping(ping) => conn.sender.pong(ping.arg()).await.unwrap(),
                Message::Privmsg(message) => {
                    if let Err(e) = tx.try_send(message) {
                        log::error!("The writer thread must have panicked as its queue is unavailable: {e}. Shutting down the listener.");
                        break;
                    }
                },
                _ => ()
            },
            Err(err) => {
                panic!("{}", err);
            }
        }
    }
  }

  Ok(())
}

pub fn main() -> Result<(), anyhow::Error> {
  env_logger::init();

  let path = std::env::args().nth(1);
  let path = path.as_ref().map(|s| &s[..]).unwrap_or(DEFAULT_CONFIG_PATH);
  let path = std::path::Path::new(path);

  let config = match std::fs::read_to_string(path) {
    Ok(s) => serde_json::from_str(&s)?,
    Err(_) => {
      log::warn!("Couldn't read the config file, falling back to the default one.");
      CollectorConfig {
        channels: vec!["moscowwbish".to_string(), "ambadev".to_string()],
        output_directory: default_output_directory(),
      }
    }
  };

  if config.channels.is_empty() {
    log::error!("No channels specified in the config file, exiting.");
    anyhow::bail!("No channel specified");
  }

  if !config.output_directory.exists() {
    log::info!("Creating the output directory...");
    std::fs::create_dir_all(&config.output_directory)?;
  }

  if !config.output_directory.is_dir() {
    log::error!("The config directory is not a directory.");
    anyhow::bail!(format!("{} is not a directory", config.output_directory.display()));
  }

  log::info!("Using this config: {config:?}");

  tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap()
    .block_on(run(config))
}
