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

  pub async fn reconnect(&mut self, creds: &Credentials, channels: &[String]) -> anyhow::Result<()> {
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
          break Err(anyhow::anyhow!(format!("failed to reconnect: {e}")));
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
