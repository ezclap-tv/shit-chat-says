use actix_cors::Cors;
use actix_web::http::header;
use actix_web::{middleware, web::Data, App, HttpServer};
use std::env;

mod ctx;
mod error;
mod schema;
mod v1;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
  if std::env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "info,actix_web=debug"); // actix_web=debug enables error logging
  }
  env_logger::init();

  let model_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("..")
    .join("models");

  let ctx = ctx::Context::new(ctx::State::new(model_dir));
  let db = db::connect(("scs", "127.0.0.1", 5432, "postgres", Some("root"))).await?;

  let server = HttpServer::new(move || {
    App::new()
      .app_data(Data::new(ctx.clone()))
      .app_data(Data::new(db.clone()))
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
  server.bind("127.0.0.1:8080").unwrap().run().await?;

  Ok(())
}
