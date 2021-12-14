#![feature(hash_raw_entry)]

use std::fmt::Display;

use sqlx::PgPool;

pub use sqlx;

pub mod channels;
pub mod logs;

pub type Database = PgPool;

#[derive(Debug)]
pub struct Error(sqlx::Error);

impl From<sqlx::Error> for Error {
  fn from(e: sqlx::Error) -> Self {
    Self(e)
  }
}

impl actix_web::ResponseError for Error {
  fn status_code(&self) -> actix_web::http::StatusCode {
    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
  }

  fn error_response(&self) -> actix_web::HttpResponse {
    actix_web::HttpResponse::new(self.status_code())
  }
}

impl std::error::Error for Error {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    self.0.source()
  }
}

impl Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Internal server error")
  }
}

pub type Result<T> = std::result::Result<T, Error>;

/// * name - database name
/// * host - IP
/// * port - ...
/// * credentials - (user, password)
pub async fn connect(uri: impl Into<ConnString>) -> sqlx::Result<Database> {
  Database::connect(&uri.into().0).await
}

pub struct ConnString(String);
impl<'a> From<&'a str> for ConnString {
  fn from(v: &'a str) -> Self {
    Self(v.to_string())
  }
}
impl From<String> for ConnString {
  fn from(v: String) -> Self {
    Self(v)
  }
}
impl<'a> From<(&'a str, &'a str, i32, &'a str, Option<&'a str>)> for ConnString {
  fn from((db, host, port, user, pass): (&'a str, &'a str, i32, &'a str, Option<&'a str>)) -> Self {
    Self(match pass {
      Some(pass) => format!("postgres://{host}:{port}/{db}?user={user}&password={pass}"),
      None => format!("postgres://{host}:{port}/{db}?user={user}"),
    })
  }
}
