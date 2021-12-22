#![feature(hash_raw_entry)]

use sqlx::PgPool;

pub use sqlx;

pub mod allowlist;
pub mod channels;
pub mod logs;
pub mod tokens;
pub mod user;

pub type Database = PgPool;

pub type Result<T> = std::result::Result<T, sqlx::Error>;

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
