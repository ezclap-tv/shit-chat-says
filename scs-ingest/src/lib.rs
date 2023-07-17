use std::{
  borrow::Cow,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
};

use futures::FutureExt;

pub mod fs;
pub mod pg;
pub mod sink;

pub use db::logs::Entry;
pub use smol_str::SmolStr;

use sink::{MessageReceiver, MessageSender, RawLogRecord, Sink, SinkMessage};

#[derive(Clone, Debug)]
pub struct Channel {
  pub name: SmolStr,
  pub buffer: usize,
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to register handler for all of the OS signals. Odd.")]
pub struct NoSignalsRegistered;

pub struct SinkManager {
  should_stop: Arc<std::sync::atomic::AtomicBool>,
  sender: MessageSender,
  sinks: Vec<(Cow<'static, str>, tokio::task::JoinHandle<()>)>,
}

#[derive(Clone)]
pub struct BatchSender(MessageSender);

impl BatchSender {
  pub fn broadcast(&self, batch: Vec<RawLogRecord>) {
    let _ = self.0.send(SinkMessage::Write(batch));
  }
}

impl SinkManager {
  pub fn new(
    max_backlog_size: usize,
    flush_interval: std::time::Duration,
  ) -> Result<(Self, BatchSender), NoSignalsRegistered> {
    let (sender, _) = tokio::sync::broadcast::channel(max_backlog_size);
    let should_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let batch_sender = BatchSender(sender.clone());

    Self::spawn_signal_handler(should_stop.clone(), sender.clone())?;
    Self::spawn_flush_notifier(should_stop.clone(), sender.clone(), flush_interval);

    Ok((
      Self {
        should_stop,
        sender,
        sinks: Vec::with_capacity(1),
      },
      batch_sender,
    ))
  }

  pub fn add_sink(&mut self, sink: impl sink::Sink + Send + 'static) {
    let name = sink.name();
    log::info!("Registering a new sink '{}'", name,);
    let handle = tokio::spawn(Self::sink_supervisor(sink, self.sender.subscribe()));
    self.sinks.push((name, handle));
  }

  pub fn request_stop(&self) {
    Self::_request_stop(&self.should_stop, &self.sender);
  }

  pub async fn stop(&mut self) {
    self.request_stop();

    for (name, handle) in &mut self.sinks {
      log::info!("Waiting for sink '{}' to stop...", name);
      if let Err(e) = handle.await {
        log::error!("Sink '{}' failed to complete gracefully: {}", name, e);
      }
    }
  }

  fn _request_stop(should_stop: &AtomicBool, sender: &MessageSender) {
    let was_running = should_stop
      .fetch_update(
        std::sync::atomic::Ordering::SeqCst,
        std::sync::atomic::Ordering::SeqCst,
        |_| Some(true),
      )
      .expect("Closure always returns Some so the result is always Ok");

    if was_running {
      let _ = sender.send(SinkMessage::MustFlushAndStop);
    }
  }

  fn spawn_flush_notifier(should_stop: Arc<AtomicBool>, sender: MessageSender, flush_interval: std::time::Duration) {
    tokio::spawn(async move {
      log::info!(
        "Spawned a flush notifier (interval = {:.3}s)",
        flush_interval.as_secs_f64()
      );
      let mut instant = std::time::Instant::now();
      while !should_stop.load(Ordering::SeqCst) {
        tokio::time::sleep(flush_interval).await;
        log::info!(
          "Broadcasting a flush message to all sinks after {:.2}s",
          instant.elapsed().as_secs_f64()
        );
        let _ = sender.send(SinkMessage::Flush);
        instant = std::time::Instant::now();
      }
    });
  }

  #[cfg(target_family = "unix")]
  fn spawn_signal_handler(should_stop: Arc<AtomicBool>, sender: MessageSender) -> Result<(), NoSignalsRegistered> {
    fn try_add(
      name: &str,
      signal: Result<tokio::signal::unix::Signal, std::io::Error>,
      signals: &mut Vec<tokio::signal::unix::Signal>,
    ) {
      match signal {
        Ok(s) => signals.push(s),
        Err(e) => log::error!("Failed to register signal handler for {}: {}", name, e),
      }
    }
    let mut signals = Vec::with_capacity(4);

    // SIGTERM for docker-compose down
    let sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate());
    try_add("SIGTERM", sigterm, &mut signals);

    // SIGINT for ctrl-c
    let sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt());
    try_add("SIGINT", sigint, &mut signals);

    // SIGQUIT is Like SIGINT, but dumps the process core. Handling just in case.
    let sigquit = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::quit());
    try_add("SIGQUIT", sigquit, &mut signals);

    // SIGHUP for terminal disconnects
    let sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup());
    try_add("SIGHUP", sighup, &mut signals);

    if signals.is_empty() {
      log::error!("Failed to register all of the signal hadnlers. Aborting.");
      return Err(NoSignalsRegistered);
    }

    tokio::spawn(async move {
      log::info!("Signal task spawned");

      futures::future::select_all(signals.iter_mut().map(|s| s.recv().boxed())).await;

      log::info!("Received one of the termination signals. Notifying the supervisors to stop...");
      Self::_request_stop(&should_stop, &sender);
    });

    Ok(())
  }

  #[cfg(target_family = "windows")]
  fn spawn_signal_handler(should_stop: Arc<AtomicBool>, sender: MessageReceiver) -> Result<(), NoSignalsRegistered> {
    tokio::spawn(async move {
      log::info!("Signal task spawned");

      tokio::signal::ctrl_c().await;

      log::info!("Received the stop signal. Notifying the supervisors to stop...");
      Self::_request_stop(&should_stop, &sender);
    });
  }

  async fn sink_supervisor(mut sink: impl Sink + Send + 'static, mut rx: MessageReceiver) {
    log::info!("[SINK:{}] Supervisor started. Listening for messages...", sink.name());
    loop {
      match rx.recv().await {
        Ok(message) => match message {
          SinkMessage::Write(batch) => {
            if let Err(e) = sink.handle_messages(batch).await {
              log::error!("[SINK:{}] Error while handling messages: {}", sink.name(), e);
            }
          }
          SinkMessage::Flush => {
            log::info!("[SINK:{}] Handling a new message: {}", sink.name(), message);
            if let Err(e) = sink.flush().await {
              log::error!("[SINK:{}] Error while flushing: {}", sink.name(), e);
            } else {
              log::info!("[SINK:{}] Successfully flushed", sink.name());
            }
          }
          SinkMessage::MustFlushAndStop => {
            log::info!("[SINK:{}] Handling a new message: {}", sink.name(), message);
            log::info!("[SINK:{}] Attempting to flush and stop", sink.name());
            if let Err(e) = sink.must_flush().await {
              log::info!("[SINK:{}] Error while terminating: {}", sink.name(), e);
            } else {
              log::info!("[SINK:{}] Successfully flushed before terminating", sink.name());
            }
            break;
          }
        },
        Err(e) => match e {
          tokio::sync::broadcast::error::RecvError::Closed => break,
          tokio::sync::broadcast::error::RecvError::Lagged(missed) => {
            log::warn!(
              "[SINK:{}] Lagging behind the other sinks. Permanently lost {} messages since last receive.",
              sink.name(),
              missed
            );
          }
        },
      }
    }
    log::info!("[SINK:{}] Successfully terminated sink task", sink.name());
  }
}
