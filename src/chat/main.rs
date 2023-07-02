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
use twitch::Command;

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

struct State {
  model: Box<dyn chain::TextGenerator>,
  credentials: twitch_api::Credentials,
  cooldowns: Cooldowns,
  reply_times: HashMap<String, ChannelReplyTracker>,
  prefix: String,
  command_prefix: String,
  config: Config,
}

async fn run(config: Config) -> Result<()> {
  log::info!("Loading model");

  let mut state = State {
    model: chain::load_chain_of_any_supported_order(&config.model_path)?,
    cooldowns: Cooldowns::new(&config.channels, config.user_cooldown),
    credentials: twitch_api::Credentials::from(&config),
    reply_times: HashMap::new(),
    prefix: format!("@{}", config.login.to_ascii_lowercase()),
    command_prefix: format!("${}", config.login.to_ascii_lowercase()),
    config,
  };

  'stop: loop {
    log::info!("Connecting to Twitch");
    let mut conn = twitch_api::TwitchStream::new().await?;
    let mut error_count = 0;

    let mut reply_times = std::collections::HashMap::with_capacity(state.config.channels.len());
    for channel in &state.config.channels {
      reply_times.insert(
        channel.to_string(),
        ChannelReplyTracker {
          reply_timer: std::time::Instant::now().sub(state.config.reply_timeout),
          message_count: state.config.reply_after_messages,
        },
      );
    }
    state.reply_times = reply_times;
    conn.init(&state.credentials, &state.config.channels).await?;

    log::info!("Chat bot is ready");

    loop {
      tokio::select! {
        _ = stop_signal() => {
          log::info!("Process terminated");
          break 'stop Ok(());
        },
        result = conn.receive() => match result {
          Ok(Some(batch)) => {
              for twitch_msg in batch.lines().map(twitch::Message::parse).filter_map(Result::ok) {
                match twitch_msg.command() {
                  Command::Ping => conn.pong().await?,
                  Command::Reconnect => conn.reconnect(&state.credentials, &state.config.channels).await?,
                  Command::Privmsg => {
                    let channel = twitch_msg.channel().unwrap_or("???");
                    let login = twitch_msg.prefix().and_then(|v| v.nick).unwrap_or("???");
                    let text = twitch_msg.text().unwrap_or("???").trim();
                    let badges = twitch_msg.tag(twitch::Tag::Badges).unwrap_or("");

                    handle_message(&mut conn, &mut state, channel.strip_prefix('#').unwrap_or(channel), MessageUser {
                      login,
                      badges
                    }, text).await?;
                  },
                  _ => (),
                }
              }
          },
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
}

struct MessageUser<'a> {
  login: &'a str,
  badges: &'a str,
}

impl<'a> MessageUser<'a> {
  pub fn has_badge(&self, badge: &str) -> bool {
    self.badges.contains(badge)
  }
  pub fn is_mod(&self) -> bool {
    self.has_badge("moderator")
  }
  pub fn is_streamer(&self) -> bool {
    self.has_badge("broadcaster")
  }
}

async fn handle_message(
  conn: &mut twitch_api::TwitchStream,
  state: &mut State,
  channel: &str,
  user: MessageUser<'_>,
  text: &str,
) -> Result<()> {
  log::info!("[{channel}] {}: {text}", user.login);

  // format: `@LOGIN <seed> <...rest>`
  // `rest` is ignored

  if text.to_ascii_lowercase().starts_with(&state.prefix)
    && (user.is_mod() || user.is_streamer() || !state.cooldowns.has_cd(channel, user.login))
  {
    if state.config.reply_blocklist.contains(&user.login.to_ascii_lowercase()) {
      return Ok(());
    }

    let words = text.split_whitespace().skip(1).collect::<Vec<_>>();
    let response = match words.len() {
      0 => chain::sample(&state.model, "", MAX_SAMPLES),
      1 => chain::sample(&state.model, words[0], MAX_SAMPLES),
      _ => chain::sample_seq(&state.model, &words, MAX_SAMPLES_FOR_SEQ_INPUT),
    };
    if !response.is_empty() {
      conn.respond(channel, &response).await?;
      state.cooldowns.set_cd(channel, user.login);
    }

    return Ok(());
  }

  if text.to_ascii_lowercase().starts_with(&state.command_prefix) {
    match text.split_whitespace().nth(1) {
      Some("version") => {
        conn
          .respond(channel, &format!("SCS v{}", env!("CARGO_PKG_VERSION")))
          .await?;
      }
      Some("model") => {
        // Save to unwrap the filename here since the model has been successfully loaded.
        let model_name = state.config.model_path.file_name().unwrap();
        let model_snapshot = state
          .config
          .model_path
          .metadata()
          .and_then(|m| m.modified())
          .map(|time| {
            chrono::DateTime::<chrono::Local>::from(time)
              .with_timezone(&chrono::Utc)
              .format("%F")
              .to_string()
          })
          .unwrap_or_else(|_| String::from("unknown"));
        let model_metadata = state.model.model_meta_data();
        conn
          .respond(
            channel,
            &format!(
              "{} (version: {}; metadata: {})",
              model_name.to_string_lossy(),
              model_snapshot,
              if model_metadata.is_empty() {
                "none"
              } else {
                model_metadata
              }
            ),
          )
          .await?;
      }
      Some("?") => {
        let words = text.split_whitespace().skip(2).collect::<Vec<_>>();
        if !words.is_empty() {
          let word_metadata = state.model.phrase_meta_data(&words);
          conn.respond(channel, &word_metadata.replace('\n', " ")).await?;
        }
      }
      Some(_) | None => (),
    }
    return Ok(());
  }

  if let Some(tracker) = state.reply_times.get_mut(channel) {
    tracker.count_message();
    if !tracker.should_reply(&state.config) || state.config.reply_blocklist.contains(&user.login.to_ascii_lowercase()) {
      return Ok(());
    }

    let prob = rand::thread_rng().gen_range(0.0..1f64);
    if state.config.reply_probability > 0.0 {
      log::info!(
        "[{channel}] [=REPLY MODE=] Rolled {prob} vs {}",
        state.config.reply_probability
      );
    }
    if prob >= state.config.reply_probability {
      tracker.after_reply();
      return Ok(());
    }

    let words = text.split_whitespace().collect::<Vec<_>>();
    let response = match words.len() {
      1 => chain::sample(&state.model, words[0], MAX_SAMPLES),
      _ => chain::sample_seq(&state.model, &words, MAX_SAMPLES_FOR_SEQ_INPUT),
    };

    if !response.is_empty() && response != text.trim() && !text.starts_with(&response) {
      tracker.after_reply();
      conn.respond(channel, &format!("@{} {response}", user.login)).await?;
    }
  }

  Ok(())
}

const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[tokio::main]
async fn main() -> Result<()> {
  if env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "INFO");
  }
  env_logger::try_init()?;

  let mut config = Config::load(
    env::args()
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
