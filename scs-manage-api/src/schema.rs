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

#[derive(Clone, serde::Serialize)]
pub enum OutputKind {
  Stdout,
  Stderr,
}

#[derive(Clone, serde::Serialize)]
pub struct CommandOutput {
  pub output: String,
  pub output_kind: OutputKind,
}

#[derive(Clone, serde::Serialize)]
pub struct CommandResult {
  pub is_success: bool,
  pub status_line: String,
}

#[derive(Clone, serde::Serialize)]
pub enum CommandLine {
  Output(CommandOutput),
  Result(CommandResult),
}

#[derive(serde::Serialize)]
pub struct LastCommand {
  pub in_progress: bool,
  pub command_output: Vec<CommandLine>,
  pub last_command: Option<std::borrow::Cow<'static, str>>,
}

#[derive(serde::Serialize)]
pub struct Service {
  pub name: String,
  pub is_running: bool,
}

impl From<CommandResult> for CommandLine {
  fn from(result: CommandResult) -> Self {
    CommandLine::Result(result)
  }
}
impl From<CommandOutput> for CommandLine {
  fn from(output: CommandOutput) -> Self {
    CommandLine::Output(output)
  }
}
