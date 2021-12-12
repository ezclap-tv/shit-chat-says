use sqlx::PgPool;

pub mod logs;

pub type Database = PgPool;

///
/// * name - database name
/// * host - IP
/// * port - ...
/// * credentials - (user, password)
pub async fn connect(name: &str, host: &str, port: i32, credentials: Option<(&str, &str)>) -> sqlx::Result<Database> {
  let credentials = match credentials {
    Some((user, password)) => format!("?user={}&password={}", user, password),
    None => String::new(),
  };
  Database::connect(&format!("postgres://{}:{}/{}{}", host, port, name, credentials)).await
}
