mod config;

use anyhow::Result;
use config::Config;
use rand::Rng;
use std::{
  collections::HashMap,
  env,
  ops::Sub,
  path::PathBuf,
  time::{Duration, Instant},
};
use twitch::tmi::Message;

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

struct Cooldowns {
  last_sent: HashMap<String, HashMap<String, Instant>>,
  last_eviction: Instant,
  cd: Duration,
}
impl Cooldowns {
  pub fn new(channels: &[String], cd: Duration) -> Self {
    let mut last_sent = HashMap::with_capacity(channels.len());
    for channel in channels {
      last_sent.insert(channel.clone(), HashMap::new());
    }
    Self {
      last_sent,
      last_eviction: Instant::now(),
      cd,
    }
  }

  pub fn has_cd(&mut self, channel: &str, user: &str) -> bool {
    // regularly evict users
    if self.last_eviction.elapsed() > self.cd {
      for (_, ch) in self.last_sent.iter_mut() {
        ch.retain(|k, v| {
          if cfg!(debug_assertions) && v.elapsed() >= self.cd {
            log::info!("{} cooldown expired", k);
          }
          v.elapsed() < self.cd
        });
      }
    }

    // no need to evict here, even if they weren't evicted now, they will be next time
    !self
      .last_sent
      .get(channel)
      .and_then(|v| v.get(user))
      .map(|v| v.elapsed() > self.cd)
      .unwrap_or(true)
  }

  pub fn set_cd(&mut self, channel: &str, user: &str) {
    if cfg!(debug_assertions) {
      log::info!("Replied to {}", user);
    }
    if let Some(ch) = self.last_sent.get_mut(channel) {
      ch.insert(user.to_string(), Instant::now());
    }
  }
}

async fn run(config: Config) -> Result<()> {
  log::info!("Loading model");
  let model = chain::load_chain_of_any_supported_order(&config.model_path)?;
  let mut cds = Cooldowns::new(&config.channels, config.user_cooldown);

  'stop: loop {
    log::info!("Connecting to Twitch");
    let mut conn = twitch::tmi::connect(config.clone().into()).await.unwrap();

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

    log::info!("Chat bot is ready");

    loop {
      tokio::select! {
        _ = stop_signal() => {
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
              let user = message.user.login();
              if text.to_ascii_lowercase().starts_with(&prefix) && (message.user.is_mod() || message.user.is_streamer() || !cds.has_cd(channel, user)) {
                let words = text.split_whitespace().skip(1).collect::<Vec<_>>();
                let response = match words.len() {
                  0 => chain::sample(&model, "", MAX_SAMPLES),
                  1 => chain::sample(&model, words[0], MAX_SAMPLES),
                  _ => chain::sample_seq(&model, &words, MAX_SAMPLES_FOR_SEQ_INPUT),
                };
                if !response.is_empty() {
                  conn.sender.privmsg(channel, &response).await?;
                  cds.set_cd(channel, user);
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
                  if !tracker.should_reply(&config) || config.reply_blocklist.contains(&login.to_ascii_lowercase()) {
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
          Err(twitch::tmi::conn::Error::StreamClosed) => break,
          // fatal error
          Err(_) => break 'stop Ok(()),
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
