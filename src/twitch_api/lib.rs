use std::time::Duration;

use anyhow::Result;
use futures::{SinkExt, StreamExt};

use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

pub mod credentials;

pub use credentials::Credentials;
pub type WsError = tokio_tungstenite::tungstenite::Error;

pub struct TwitchStream {
  uri: String,
  ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
  smb: SameMessageBypass,
}

impl TwitchStream {
  pub async fn new() -> Result<Self, WsError> {
    Self::with_uri("ws://irc-ws.chat.twitch.tv:80").await
  }

  pub async fn with_uri(uri: impl Into<String>) -> Result<Self, WsError> {
    let uri = uri.into();
    let (ws, _) = tokio_tungstenite::connect_async(&uri).await?;
    Ok(Self {
      ws,
      uri,
      smb: SameMessageBypass::default(),
    })
  }

  pub async fn init(&mut self, credentials: &Credentials, channels: &[String]) -> Result<(), WsError> {
    self.authenticate(credentials).await?;
    self.join(channels).await?;
    Ok(())
  }

  pub async fn authenticate(&mut self, credentials: &Credentials) -> Result<(), WsError> {
    let (login, token) = credentials.get();

    log::info!("Authenticating as {}...", login);
    self.send("CAP REQ :twitch.tv/commands twitch.tv/tags").await?;
    self.send(format!("PASS {token}")).await?;
    self.send(format!("NICK {login}")).await?;

    Ok(())
  }

  pub async fn join(&mut self, channels: &[String]) -> Result<(), WsError> {
    log::info!("Joining channels: {}", channels.join(", "));

    self
      .send(format!(
        "JOIN {}",
        channels.iter().map(|c| format!("#{c}")).collect::<Vec<_>>().join(",")
      ))
      .await
  }

  pub async fn respond(&mut self, channel: &str, content: &str) -> Result<(), WsError> {
    let text = format!("PRIVMSG #{} :{}{}\r\n", channel, content, self.smb.get());
    self.send(text).await
  }

  pub async fn receive(&mut self) -> Result<Option<String>, WsError> {
    match self.ws.next().await {
      Some(Ok(Message::Text(batch))) => Ok(Some(batch)),
      Some(Ok(_)) | None => Ok(None),
      Some(Err(e)) => Err(e),
    }
  }

  pub async fn pong(&mut self) -> Result<(), WsError> {
    self.send("PONG").await
  }

  pub async fn reconnect(&mut self, creds: &Credentials, channels: &[String]) -> std::result::Result<(), WsError> {
    let mut tries = 10;
    let mut delay = Duration::from_secs(3);

    log::info!("> Reconnecting");
    tokio::time::sleep(delay).await;

    loop {
      let mut new_stream = Self::with_uri(self.uri.clone()).await?;
      match new_stream.init(creds, channels).await {
        Ok(_) => {
          *self = new_stream;
          break Ok(());
        }
        Err(e) if tries > 0 => {
          tries -= 1;
          delay *= 3;
          log::info!("> Connection failed: {}", e);
          log::info!("> Retrying...");
          tokio::time::sleep(delay).await;
          continue;
        }
        Err(e) => {
          log::warn!("Failed to reconnect: {}", e);
          break Err(e);
        }
      }
    }
  }

  async fn send(&mut self, msg: impl Into<String>) -> Result<(), WsError> {
    self.ws.send(Message::Text(msg.into())).await
  }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SameMessageBypass {
  flag: u8,
}
impl SameMessageBypass {
  pub fn get(&mut self) -> &'static str {
    match self.flag {
      0 => {
        self.flag = 1;
        ""
      }
      1 => {
        self.flag = 0;
        concat!(" ", "⠀")
      }
      _ => unreachable!(),
    }
  }
}

#[allow(clippy::derivable_impls)]
impl Default for SameMessageBypass {
  fn default() -> Self {
    SameMessageBypass { flag: 0 }
  }
}

#[derive(Debug, Clone, Copy)]
pub enum SuggestedAction {
  KeepGoing,
  Reconnect,
  Terminate,
}

impl<'a> From<&'a WsError> for SuggestedAction {
  fn from(e: &'a WsError) -> Self {
    match e {
      // We've received or sent a message that's too large. Extremely unlikely to happen.
      tokio_tungstenite::tungstenite::Error::Capacity(_) => SuggestedAction::KeepGoing,
      // The queue is unlimited by default, so this shouldn't happen.
      tokio_tungstenite::tungstenite::Error::SendQueueFull(_) => SuggestedAction::KeepGoing,
      // Can't really do anything about this, so just keep going.
      tokio_tungstenite::tungstenite::Error::Utf8 => SuggestedAction::KeepGoing,
      // This shouldn't happen, because the stream returns `None` once closed.
      tokio_tungstenite::tungstenite::Error::ConnectionClosed
      | tokio_tungstenite::tungstenite::Error::AlreadyClosed => SuggestedAction::Reconnect,
      // Twitch isn't following the websocket protocol. Unlikely to happen.
      tokio_tungstenite::tungstenite::Error::Protocol(_) => SuggestedAction::Reconnect,
      // We've received an HTTP error while trying to upgrade the websocket connection. Unlikely to happen.
      tokio_tungstenite::tungstenite::Error::Http(_) => SuggestedAction::Reconnect,
      // This one covers a few errors, including badly formatted status codes and headers. Unlikely to happen.
      tokio_tungstenite::tungstenite::Error::HttpFormat(_) => SuggestedAction::Reconnect,
      // IO indicates a terminal error like DNS resolution failure or a broken socket.
      tokio_tungstenite::tungstenite::Error::Io(_) => SuggestedAction::Terminate,
      // URL error indicates that the URL we're trying to connect to is invalid. It's hardcoded to be valid, so this shouldn't happen.
      tokio_tungstenite::tungstenite::Error::Url(_) => SuggestedAction::Terminate,
      // TLS error, this includes protocol-level errors and other errors such as DNS errors.
      tokio_tungstenite::tungstenite::Error::Tls(_) => SuggestedAction::Terminate,
    }
  }
}

impl std::fmt::Display for SuggestedAction {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    <Self as std::fmt::Debug>::fmt(self, f)
  }
}
