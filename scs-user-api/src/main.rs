use actix_cors::Cors;
use actix_web::{self, get, http::header, middleware, web::Data, App, HttpResponse, HttpServer};
use db::ConnString;
use std::{env, path::PathBuf};
use structopt::StructOpt;

mod auth;
mod ctx;
mod error;
mod ex;
mod schema;
mod v1;

#[derive(Debug, StructOpt)]
#[structopt(name = "scs-user-api", about = "SCS User API")]
struct Options {
  #[structopt(long, env = "SCS_USER_API_HOST", default_value = "127.0.0.1")]
  host: String,
  #[structopt(long, env = "SCS_USER_API_CLIENT_SECRET")]
  secret: String,
  #[structopt(long, env = "SCS_USER_API_MODEL_DIR", parse(from_os_str))]
  model_dir: Option<PathBuf>,
}

#[derive(StructOpt)]
struct DbOptions {
  #[structopt(long, env = "SCS_DB_HOST", default_value = "localhost")]
  host: String,
  #[structopt(long, env = "SCS_DB_PORT", default_value = "5432")]
  port: u16,
  #[structopt(long, env = "SCS_DB_NAME", default_value = "scs")]
  name: String,
  #[structopt(long, env = "SCS_DB_USER", default_value = "scs")]
  user: String,
  #[structopt(long, env = "SCS_DB_PASSWORD")]
  password: Option<String>,
}

impl From<DbOptions> for ConnString {
  fn from(val: DbOptions) -> Self {
    ConnString::from((
      &val.name[..],
      &val.host[..],
      val.port,
      &val.user[..],
      val.password.as_ref().map(|s| &s[..]),
    ))
  }
}

#[get("/health")]
async fn health_check() -> HttpResponse {
  HttpResponse::Ok().finish()
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
  if std::env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "info,actix_web=debug"); // actix_web=debug enables error logging
  }
  env_logger::init();

  let options = Options::from_args_safe()?;
  let db_options = DbOptions::from_args_safe()?;

  let client_secret = auth::ClientSecret(options.secret);
  let model_dir = options.model_dir.unwrap_or_else(|| {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .join("..")
      .join("models")
  });

  let ctx = ctx::Context::new(ctx::State::new(model_dir));
  let db = db::connect(db_options).await?;

  let req_client = reqwest::Client::new();

  let server = HttpServer::new(move || {
    App::new()
      .app_data(Data::new(client_secret.clone()))
      .app_data(Data::new(ctx.clone()))
      .app_data(Data::new(db.clone()))
      .app_data(Data::new(req_client.clone()))
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
      .service(health_check)
      .service(auth::create_token)
      .service(v1::routes())
  });
  server.bind(format!("{}:8080", options.host)).unwrap().run().await?;

  Ok(())
}
