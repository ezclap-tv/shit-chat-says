#[derive(Debug, Clone)]
pub struct Regular {
  pub login: String,
  pub token: String,
}

#[derive(Debug, Clone)]
pub enum Credentials {
  Regular(Regular),
  Anonymous,
}

impl Credentials {
  /// Returns the login and token as a tuple, in this order.
  pub fn get(&self) -> (&str, &str) {
    match self {
      Credentials::Regular(r) => (&r.login[..], &r.token[..]),
      Credentials::Anonymous => ("justinfan83124", "just_a_lil_guy"),
    }
  }
}
