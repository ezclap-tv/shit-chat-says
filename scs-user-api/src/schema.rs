use chain::TextGenerator;
use chrono::{DateTime, Utc};
use serde::Serialize;

/// Information that can be gathered just by reading the filesystem
#[derive(Serialize)]
pub struct SimpleModelInfo {
  pub name: String,
  pub date_created: DateTime<Utc>,
  pub date_modified: DateTime<Utc>,
  pub size: f64,
}

/// Information that
#[derive(Serialize)]
pub struct Model {
  pub name: String,
  pub date_created: DateTime<Utc>,
  pub date_modified: DateTime<Utc>,
  pub size: f64,
  pub order: usize,
  pub channels: Vec<String>,
  #[serde(skip)]
  pub chain: Box<dyn TextGenerator>,
}

impl Model {
  pub fn simple(&self) -> SimpleModelInfo {
    SimpleModelInfo {
      name: self.name.clone(),
      date_created: self.date_created.clone(),
      date_modified: self.date_modified.clone(),
      size: self.size,
    }
  }
}
