use std::sync::Arc;

use tokio::sync::RwLock;

pub struct State {
  pub config: crate::config::Config,
  pub config_path: std::path::PathBuf,
}

impl State {
  pub fn compose_command(&self, args: impl Fn(&mut tokio::process::Command)) -> tokio::process::Command {
    compose_command(&self.config.compose_file, args)
  }
}

pub(crate) fn compose_command(
  compose_file: &std::path::Path,
  args: impl Fn(&mut tokio::process::Command),
) -> tokio::process::Command {
  command("docker-compose", move |cmd| {
    cmd.env("COMPOSE_DOCKER_CLI_BUILD", "1");
    cmd.env("DOCKER_BUILDKIT", "1");
    cmd.arg("-f").arg(compose_file);
    args(cmd);
  })
}

pub(crate) fn command<S: AsRef<std::ffi::OsStr>>(
  name: S,
  args: impl Fn(&mut tokio::process::Command),
) -> tokio::process::Command {
  let mut cmd = tokio::process::Command::new(name);
  args(&mut cmd);
  cmd
}

#[derive(Clone)]
pub struct Context(std::sync::Arc<RwLock<State>>);

impl Context {
  pub fn new(state: State) -> Self {
    Self(Arc::new(RwLock::new(state)))
  }

  pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, State> {
    self.0.read().await
  }

  pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, State> {
    self.0.write().await
  }

  pub async fn write_owned(&self) -> tokio::sync::OwnedRwLockWriteGuard<State> {
    Arc::clone(&self.0).write_owned().await
  }
}
