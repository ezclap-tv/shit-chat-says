use std::borrow::Cow;

use async_trait::async_trait;
use thiserror::Error;

/// Twitch usernames are up to 25 characters long, so we can save a bunch of allocations
/// by using SmolStr, which allows up to 24 characters without allocating.
pub type RawLogRecord = db::logs::Entry<smol_str::SmolStr>;
pub type MessageSender = tokio::sync::broadcast::Sender<SinkMessage>;
pub type MessageReceiver = tokio::sync::broadcast::Receiver<SinkMessage>;

#[async_trait]
pub trait Sink: Send {
  /// Should return the name of the sink.
  fn name(&self) -> Cow<'static, str>;
  /// Should handle the incoming batch of message.
  async fn handle_messages(&mut self, batch: Vec<RawLogRecord>) -> Result<(), SinkError>;
  /// Should flush the sink's buffers to the store. Invoked on a timer.
  async fn flush(&mut self) -> Result<(), SinkError>;
  /// Must immediately flush the sink's buffers to the store. This method will be invoked
  /// if the OS issues a kill signal.
  async fn must_flush(&mut self) -> Result<(), SinkError>;
}

#[derive(Error, Debug)]
pub enum SinkError {
  /// An IO error.
  #[error("unexpected IO error while handling a message: {0}")]
  Io(#[from] std::io::Error),
  /// A database error.
  #[error("unexpected Db error while handling a message: {0}")]
  Db(#[from] db::sqlx::Error),
  /// Some other error.
  #[error(transparent)]
  Other(#[from] anyhow::Error),
}

/// A message with a command that must be performed by the sink.
#[derive(Clone, Debug)]
pub enum SinkMessage {
  /// An incoming batch of messages.
  Write(Vec<RawLogRecord>),
  /// A suggestion to flush the sink's buffers to the store. Triggered by a timer.
  Flush,
  /// A request to immediately flush **everything** to the store, using sync APIs if necessary.
  /// This message will be sent if OS issues a kill signal to the process.
  /// Sinks should additionally take care to flush their buffers on drop.
  MustFlushAndStop,
}

impl std::fmt::Display for SinkMessage {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      SinkMessage::Write(b) => f.debug_tuple("SinkMessage::Write").field(&b.len()).finish(),
      SinkMessage::Flush => f.debug_tuple("SinkMessage::flush").finish(),
      SinkMessage::MustFlushAndStop => f.debug_tuple("SinkMessage::MustFlushAndStop").finish(),
    }
  }
}
