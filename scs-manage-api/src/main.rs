use std::env;

use actix_cors::Cors;
use actix_web::http::header;
use actix_web::{middleware, web::Data, App, HttpServer};

mod config;
pub mod ctx;
mod schema;
mod streaming;
mod v1;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
  if std::env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "info");
  }
  env_logger::init();

  let config_path = env::args()
    .nth(1)
    .map(std::path::PathBuf::from)
    .unwrap_or_else(|| {
      std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("config")
        .join("ci-api.json")
    })
    .canonicalize()?;
  let config = config::Config::load(&config_path)?;

  log::info!("Using the following configuration: {:?}", &config);

  log::info!("Changing the directory to {}", config.project_source_folder.display());
  std::env::set_current_dir(&config.project_source_folder)?;

  let ctx = ctx::Context::new(ctx::State { config, config_path });

  let server = HttpServer::new(move || {
    App::new()
      .app_data(Data::new(ctx.clone()))
      .wrap(
        Cors::default()
          .allow_any_origin()
          .allowed_methods(vec!["POST", "GET"])
          .allowed_headers(vec![header::AUTHORIZATION, header::ACCEPT])
          .allowed_header(header::CONTENT_TYPE)
          .supports_credentials()
          .max_age(3600),
      )
      .wrap(middleware::Compress::default())
      .wrap(middleware::Logger::default())
      .service(v1::routes())
  });
  server.bind("127.0.0.1:7191").unwrap().run().await?;
  Ok(())
}
