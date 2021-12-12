use serde::Serialize;

#[derive(Serialize)]
pub struct SCSConfig {
  pub name: String,
  pub contents: String,
}

#[derive(Serialize)]
pub struct ConfigList {
  pub configs: Vec<SCSConfig>,
}

#[derive(serde::Serialize)]
pub enum OutputKind {
  Stdout,
  Stderr,
}

#[derive(serde::Serialize)]
pub struct CommandOutput {
  pub output: String,
  pub output_kind: OutputKind,
}

#[derive(serde::Serialize)]
pub struct CommandResult {
  pub is_success: bool,
  pub status_line: String,
}
