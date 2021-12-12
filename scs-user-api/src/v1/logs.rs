use super::ctx::Context;
use actix_web::{get, web, Responder, Result};
use serde::{Deserialize, Serialize};

#[get("/logs")]
pub async fn get_channel_list(ctx: web::Data<Context>) -> Result<impl Responder> {
  Ok(web::Json(ctx.read().await.get_logged_channels().await?))
}

#[derive(Debug, Deserialize)]
pub struct ChannelLogsQuery {
  pub query: Option<String>,
  pub page: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChannelLogsResponse {
  pub messages: String,
  pub page: String,
}

#[get("/logs/{channel}")]
pub async fn get_channel_logs(
  ctx: web::Data<Context>,
  channel: web::Path<String>,
  query: web::Query<ChannelLogsQuery>,
) -> Result<impl Responder> {
  log::info!("channel {:?}, query {:?}", channel, query.0);
  Ok(web::Json(
    ctx
      .read()
      .await
      .get_logs(&*channel, query.page.as_deref())
      .await?
      .map(|(messages, page)| ChannelLogsResponse { messages, page }),
  ))
}
