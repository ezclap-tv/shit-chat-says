use actix_web::{web, Scope};

pub mod logs;
pub mod models;

pub fn routes() -> Scope {
  web::scope("/v1")
    .service(logs::get_channel_list)
    .service(logs::get_channel_logs)
    .service(models::get_models_list)
    .service(models::get_model)
    .service(models::get_model_edges)
    .service(models::get_model_generated_text)
}
