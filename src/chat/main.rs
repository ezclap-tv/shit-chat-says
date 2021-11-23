#![feature(format_args_capture)]

mod config;

use anyhow::Result;
use config::Config;
use rand::Rng;
use std::{env, ops::Sub, path::PathBuf};
use twitch::Message;

// Set to 0 to disable sampling.
const MAX_SAMPLES: usize = 4;
const MAX_SAMPLES_FOR_SEQ_INPUT: usize = 16;

struct ChannelReplyTracker {
  reply_timer: std::time::Instant,
  message_count: usize,
}
impl ChannelReplyTracker {
  fn count_message(&mut self) {
    self.message_count += 1;
  }

  fn after_reply(&mut self) {
    self.message_count = 0;
    self.reply_timer = std::time::Instant::now();
  }

  fn should_reply(&self, config: &Config) -> bool {
    self.reply_timer.elapsed() >= config.reply_timeout && self.message_count >= config.reply_after_messages
  }
}

async fn run(config: Config) -> Result<()> {
  log::info!("Loading model");
  let model = chain::load_chain_of_any_supported_order(&config.model_path)?;

  'stop: loop {
    log::info!("Connecting to Twitch");
    let mut conn = twitch::connect(config.clone().into()).await.unwrap();

    let mut reply_times = std::collections::HashMap::with_capacity(config.channels.len());
    for channel in &config.channels {
      log::info!("Joining channel '{}'", channel);
      conn.sender.join(channel).await?;
      reply_times.insert(
        channel.to_string(),
        ChannelReplyTracker {
          reply_timer: std::time::Instant::now().sub(config.reply_timeout),
          message_count: config.reply_after_messages,
        },
      );
    }

    let prefix = format!("@{}", config.login.to_ascii_lowercase());
    let command_prefix = format!("${}", config.login.to_ascii_lowercase());

    #[cfg(target_os = "windows")]
    let stop = tokio::signal::ctrl_c();
    #[cfg(not(target_os = "windows"))]
    let stop = tokio::join!(
      tokio::signal::signal(tokio::signal::SignalKind::terminate()), // SIGTERM for docker-compose down
      tokio::signal::signal(tokio::signal::SignalKind::interrupt())  // SIGINT for ctrl-c
    );

    log::info!("Chat bot is ready");

    loop {
      tokio::select! {
        _ = stop => {
          log::info!("Process terminated");
          break 'stop Ok(());
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
                let words = text.split_whitespace().skip(1).collect::<Vec<_>>();
                let response = match words.len() {
                  0 => chain::sample(&model, "", MAX_SAMPLES),
                  1 => chain::sample(&model, words[0], MAX_SAMPLES),
                  _ => chain::sample_seq(&model, &words, MAX_SAMPLES_FOR_SEQ_INPUT),
                };
                if !response.is_empty() {
                  conn.sender.privmsg(channel, &response).await?;
                }
              } else if text.to_ascii_lowercase().starts_with(&command_prefix) {
                match text.split_whitespace().nth(1) {
                  Some("version") => {
                    conn.sender.privmsg(channel, &format!("SCS v{}", env!("CARGO_PKG_VERSION"))).await?;
                  }
                  Some("model") => {
                    // Save to unwrap the filename here since the model has been successfully loaded.
                    let model_name = config.model_path.file_name().unwrap();
                    let model_snapshot = config.model_path.metadata().and_then(|m| m.modified()).map(|time| {
                      chrono::DateTime::<chrono::Local>::from(time).with_timezone(&chrono::Utc).format("%F").to_string()
                    }).unwrap_or_else(|_| String::from("unknown"));
                    let model_metadata = model.model_meta_data();
                    conn.sender.privmsg(
                      channel,
                      &format!(
                        "{} (version: {}; metadata: {})",
                        model_name.to_string_lossy(),
                        model_snapshot,
                        if model_metadata.is_empty() { "none" } else { model_metadata }
                      )
                    ).await?;
                  },
                  Some("?") => {
                    let words = text.split_whitespace().skip(2).collect::<Vec<_>>();
                    if !words.is_empty() {
                      let word_metadata = model.phrase_meta_data(&words);
                      conn.sender.privmsg(
                        channel,
                        &word_metadata.replace("\n", " "),
                      ).await?;
                    }
                  }
                  Some(_) | None => ()
                }
              } else if let Some(tracker) = reply_times.get_mut(channel)
                {
                  tracker.count_message();
                  if !tracker.should_reply(&config) {
                    continue;
                  }

                  let prob = rand::thread_rng().gen_range(0.0..1f64);
                  if config.reply_probability > 0.0 {
                    log::info!("[{channel}] [=REPLY MODE=] Rolled {prob} vs {}", config.reply_probability);
                  }
                  if prob >= config.reply_probability {
                    tracker.after_reply();
                    continue;
                  }


                  let words = text.split_whitespace().collect::<Vec<_>>();
                  let response = match words.len() {
                    1 => chain::sample(&model, words[0], MAX_SAMPLES),
                    _ => chain::sample_seq(&model, &words, MAX_SAMPLES_FOR_SEQ_INPUT),
                  };

                  if !response.is_empty() && response != text.trim() && !text.starts_with(&response) {
                    tracker.after_reply();
                    conn.sender.privmsg(channel, &format!("@{login} {response}")).await?;
                  }
                }
            },
            _ => ()
          },
          // recoverable error, reconnect
          Err(twitch::conn::Error::StreamClosed) => break;
          // fatal error
          Err(err) => break 'stop;
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
