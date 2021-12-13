pub struct Error {
  inner: anyhow::Error,
}
impl actix_web::error::ResponseError for Error {
  // TODO: downcast inner
}
impl From<anyhow::Error> for Error {
  fn from(inner: anyhow::Error) -> Error {
    Error { inner }
  }
}

impl std::fmt::Debug for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    <anyhow::Error as std::fmt::Debug>::fmt(&self.inner, f)
  }
}

impl std::fmt::Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    <anyhow::Error as std::fmt::Display>::fmt(&self.inner, f)
  }
}

pub trait IntoActixResult<T> {
  fn to_actix(self) -> std::result::Result<T, Error>;
}

impl<T> IntoActixResult<T> for anyhow::Result<T> {
  fn to_actix(self) -> std::result::Result<T, Error> {
    self.map_err(|inner| Error { inner })
  }
}
