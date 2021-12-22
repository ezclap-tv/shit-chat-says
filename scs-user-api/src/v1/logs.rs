use crate::auth;
use crate::error::FailWith;
use actix_web::{get, web, Responder, Result};
use db::{self, Database};
use serde::{Deserialize, Serialize};

pub const MAX_PAGE_SIZE: u32 = 1024;
pub const DEFAULT_PAGE_SIZE: u32 = 128;

#[get("/logs/channels")]
pub async fn get_channel_list(_: auth::AccessToken, db: web::Data<Database>) -> Result<impl Responder> {
  let channels = db::channels::get_logged_channels(db.get_ref()).await.internal()?;
  Ok(web::Json(channels))
}

#[derive(Debug, Deserialize)]
pub struct ChannelLogsQuery {
  pub chatter: Option<String>,
  pub pattern: Option<String>,
  pub cursor: Option<String>,
  pub page_size: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ChannelsResponse<T> {
  pub messages: Vec<db::logs::Entry<T>>,
  pub cursor: Option<String>,
}

#[get("/logs/{channel}")]
pub async fn get_channel_logs(
  _: auth::AccessToken,
  db: web::Data<Database>,
  channel: web::Path<String>,
  query: web::Query<ChannelLogsQuery>,
) -> Result<impl Responder> {
  let ChannelLogsQuery {
    chatter,
    pattern,
    cursor,
    page_size,
  } = query.0;

  let cursor = parse_cursor(cursor)?;

  let messages = db::logs::fetch_logs_paged_with_usernames(
    db.get_ref(),
    channel.into_inner(),
    chatter,
    pattern,
    page_size.unwrap_or(DEFAULT_PAGE_SIZE).min(MAX_PAGE_SIZE),
    cursor,
  )
  .await
  .internal()?;
  let cursor = generate_cursor(&messages);
  Ok(web::Json(ChannelsResponse { messages, cursor }))
}

fn parse_cursor(cursor: Option<String>) -> Result<Option<(i64, chrono::DateTime<chrono::Utc>)>> {
  Ok(if let Some(c) = cursor {
    if c.is_empty() {
      return Ok(None);
    }
    let bytes = base64::decode_config(c, base64::URL_SAFE)
      .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid cursor: {}", e)))?;
    let decoded = String::from_utf8(bytes)
      .map_err(|e| actix_web::error::ErrorBadRequest(format!("Cursor is not a valid utf-8 string: {}", e)))?;
    let (id, sent_at) = decoded
      .split_once(",")
      .ok_or_else(|| actix_web::error::ErrorBadRequest("Cursor string is not correctly formatted"))?;
    let id = id
      .parse::<i64>()
      .map_err(|e| actix_web::error::ErrorBadRequest(format!("Channel ID cursor is not a valid number: {}", e)))?;
    let sent_at = chrono::DateTime::<chrono::FixedOffset>::parse_from_rfc3339(sent_at)
      .map_err(|e| {
        actix_web::error::ErrorBadRequest(format!("sent_at cursor is not a valid RFC3339 date string: {}", e))
      })?
      .with_timezone(&chrono::Utc);
    Some((id, sent_at))
  } else {
    None
  })
}

fn generate_cursor<T>(messages: &[db::logs::Entry<T>]) -> Option<String> {
  messages.last().map(|msg| {
    let cursor = format!("{},{}", msg.id(), msg.sent_at().to_rfc3339());
    base64::encode_config(&cursor, base64::URL_SAFE)
  })
}
