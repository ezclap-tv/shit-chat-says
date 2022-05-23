pub mod config;
pub mod sink;

use anyhow::Result;
use config::Config;
use std::env;

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

type TestMessage = (String, String, String);
async fn producer(running: std::sync::Arc<std::sync::atomic::AtomicBool>, q: tokio::sync::mpsc::Sender<TestMessage>) {
  let msg = ("channel".to_string(), "user".to_string(), "a".repeat(512));

  let overall_timer = std::time::Instant::now();
  let overall_duration = std::time::Duration::from_millis(60_000); // 1 minute;

  let mut timer = std::time::Instant::now();
  let deadline = std::time::Duration::from_millis(1000); // 1s
  let mut n_tokens: usize = 50; // 50 tokens per second

  let mut bucket_size = n_tokens;

  let mut iterations = 0;
  let mut suggested_bucket_size_acc = 0;
  let suggested_bucket_size_iters = 10;
  let mut total_consumption_rate = 0;

  'outer: while running.load(std::sync::atomic::Ordering::SeqCst) && overall_timer.elapsed() < overall_duration {
    iterations += 1;

    let mut elapsed = timer.elapsed();
    while bucket_size > 0 && elapsed < deadline {
      match q.send_timeout(msg.clone(), std::time::Duration::from_micros(10)).await {
        Ok(_) => {
          bucket_size -= 1;
        }
        Err(tokio::sync::mpsc::error::SendTimeoutError::Timeout(_)) => (),
        _ => break 'outer,
      }
      elapsed = timer.elapsed();
    }

    let consumption_rate = elapsed.as_micros() / ((n_tokens - bucket_size) as u128).max(1); // microseconds per token
    total_consumption_rate += consumption_rate;
    if elapsed >= deadline {
      log::info!(
        "[BENCH] Deadline: 1, remaining tokens: {}, rate = {}μs/token",
        bucket_size,
        consumption_rate
      );
    } else {
      let suggested_bucket_size = deadline.as_micros() / consumption_rate;
      suggested_bucket_size_acc += suggested_bucket_size;
      log::info!(
        "[BENCH] Deadline: 0, remaining time: {}ms, rate = {}μs/token, suggested_bucket_size = {}",
        (deadline - elapsed).as_millis(),
        consumption_rate,
        suggested_bucket_size,
      );
      if iterations % suggested_bucket_size_iters == 0 {
        let new_bucket_size = suggested_bucket_size_acc as f64 / suggested_bucket_size_iters as f64;
        suggested_bucket_size_acc = 0;
        log::info!(
          "[BENCH] Adjusting the bucket size: {} -> {}",
          n_tokens,
          new_bucket_size as usize
        );
        n_tokens = new_bucket_size.ceil() as usize;
      }

      tokio::time::sleep(deadline - elapsed).await;
    }

    timer = std::time::Instant::now();
    bucket_size = n_tokens;
  }

  running.store(false, std::sync::atomic::Ordering::SeqCst);

  log::info!("[BENCH] final mps = {}", n_tokens);
  log::info!(
    "[BENCH] final rate = {}μs/message",
    ((total_consumption_rate as f64) / (iterations as f64)).ceil() as u128
  );
}

async fn run(db: db::Database, mut config: Config) -> Result<()> {
  let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
  let (tx, mut rx) = tokio::sync::mpsc::channel(1);
  let _producer = tokio::spawn(producer(running.clone(), tx.clone()));

  config.channels = vec![config::Channel {
    name: "channel".to_string(),
    message_buffer_size: 1024,
  }];

  let inserter = sink::LogInserter::new(
    db.clone(),
    config.filesystem_buffer_directory.clone(),
    config.buffer_lifetime,
    &config.channels,
  )
  .await?;

  while running.load(std::sync::atomic::Ordering::SeqCst) {
    tokio::select! {
      _ = stop_signal() => {
        log::info!("Process terminated");
        running.store(false, std::sync::atomic::Ordering::SeqCst);
        break;
      },
      result = rx.recv() => match result {
        Some((channel, login, text)) => {
            inserter.insert_message(
              db::logs::UnresolvedEntry::new(channel.to_owned(), login.to_owned(), chrono::Utc::now(), text.to_owned())
            ).await?;
          },
        // fatal error
        None => continue,
      }
    }
  }

  running.store(false, std::sync::atomic::Ordering::SeqCst);
  let _ = inserter.join();

  Ok(())
}

static CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[tokio::main]
async fn main() -> Result<()> {
  if env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "INFO,sqlx=warn");
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
