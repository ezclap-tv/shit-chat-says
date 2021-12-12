use sqlx::PgPool;

pub mod logs;

pub type Database = PgPool;

/// * name - database name
/// * host - IP
/// * port - ...
/// * credentials - (user, password)
pub async fn connect(uri: impl Into<ConnString>) -> sqlx::Result<Database> {
  Database::connect(&uri.into().0).await
}

pub struct ConnString(String);
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
