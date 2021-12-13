use actix_web::{get, web, Responder, Result};
use db::{self, Database};
use serde::Deserialize;

#[get("/logs")]
pub async fn get_channel_list(db: web::Data<Database>) -> Result<impl Responder> {
  let channels = db::channels::get_all(db.get_ref()).await?;
  Ok(web::Json(channels))
}

#[derive(Debug, Deserialize)]
pub struct ChannelLogsQuery {
  pub chatter: Option<String>,
  pub pattern: Option<String>,
  pub offset: Option<i32>,
  pub limit: Option<i32>,
}

#[get("/logs/{channel}")]
pub async fn get_channel_logs(
  db: web::Data<Database>,
  channel: web::Path<String>,
  query: web::Query<ChannelLogsQuery>,
) -> Result<impl Responder> {
  let ChannelLogsQuery {
    chatter,
    pattern,
    offset,
    limit,
  } = query.0;
  let messages = db::logs::fetch_all(db.get_ref(), channel.into_inner(), chatter, pattern, offset, limit).await?;
  Ok(web::Json(messages))
}
