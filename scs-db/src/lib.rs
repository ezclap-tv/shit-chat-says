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

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn connect_and_fetch_all() {
    if std::env::var("RUST_LOG").is_err() {
      std::env::set_var("RUST_LOG", "INFO");
    }
    env_logger::init();
    let db = connect("postgres://localhost:5432/scs?user=postgres&password=root")
      .await
      .unwrap();
    for entry in logs::Entry::fetch_all(&db, "moscowwbish", Some("moscowwbish"), Some("okay%"), None, Some(10))
      .await
      .unwrap()
    {
      log::info!("{} {} {}", entry.channel(), entry.chatter(), entry.message());
    }
  }
}
