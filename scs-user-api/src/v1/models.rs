use crate::{ctx::Context, error::IntoActixResult};
use actix_web::{get, web, HttpResponse, Responder, Result};
use serde::Deserialize;

#[get("/models")]
pub async fn get_models_list(ctx: web::Data<Context>) -> Result<impl Responder> {
  use crate::error::IntoActixResult;
  let channels = ctx.write().await.get_models().await.to_actix()?;
  Ok(web::Json(channels))
}

#[get("/models/{name}")]
pub async fn get_model(ctx: web::Data<Context>, name: web::Path<String>) -> Result<impl Responder> {
  log::info!("name {:?}", name);
  Ok(HttpResponse::Ok().finish())
}

#[get("/models/{name}/{token}")]
pub async fn get_model_edges(ctx: web::Data<Context>, path: web::Path<(String, String)>) -> Result<impl Responder> {
  let (name, token) = path.into_inner();
  log::info!("name {:?}, token {:?}", name, token);
  Ok(HttpResponse::Ok().finish())
}

#[derive(Debug, Deserialize)]
pub struct ModelGenerateTextQuery {
  pub query: String,
  pub page: usize,
}

#[get("/models/{name}/{token}/generate")]
pub async fn get_model_generated_text(
  ctx: web::Data<Context>,
  path: web::Path<(String, String)>,
  query: web::Query<ModelGenerateTextQuery>,
) -> Result<impl Responder> {
  let (name, token) = path.into_inner();
  log::info!("name {:?}, token {:?}", name, token);
  Ok(HttpResponse::Ok().finish())
}
