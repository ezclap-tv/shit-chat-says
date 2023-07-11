use std::{borrow::Cow, sync::Arc};

use crossbeam_channel::{unbounded, Receiver, Sender};
use tokio::sync::RwLock;

use crate::{config::ComposeSettings, schema};

pub type Sink = Sender<schema::CommandLine>;

pub struct State {
  pub config: crate::config::Config,
  pub config_path: std::path::PathBuf,
  pub last_command: Option<Cow<'static, str>>,
  pub log_history: RwLock<Vec<schema::CommandLine>>,
  rx: Receiver<schema::CommandLine>,
  tx: Sender<schema::CommandLine>,
}

impl State {
  pub fn new(config: crate::config::Config, config_path: std::path::PathBuf) -> Self {
    let (tx, rx) = unbounded();
    Self {
      config,
      config_path,
      last_command: None,
      log_history: RwLock::new(Vec::new()),
      rx,
      tx,
    }
  }

  pub fn compose_command(&self, args: impl Fn(&mut tokio::process::Command)) -> tokio::process::Command {
    compose_command(&self.config.compose, args)
  }

  pub fn set_command<S: Into<Cow<'static, str>>>(&mut self, command: S) -> Sender<schema::CommandLine> {
    self.last_command = Some(command.into());
    self
      .log_history
      .try_write()
      .expect("This shouldn't be possible since the state is wrapped an RwLock of its own.")
      .clear();
    self.tx.clone()
  }

  pub async fn get_log_history(&self) -> Vec<schema::CommandLine> {
    let mut incoming_logs = self.rx.try_iter().collect::<Vec<_>>();
    let mut lock = self.log_history.write().await;
    lock.append(&mut incoming_logs);
    Vec::clone(&*lock)
  }
}

pub(crate) fn compose_command(
  settings: &ComposeSettings,
  args: impl Fn(&mut tokio::process::Command),
) -> tokio::process::Command {
  command("docker-compose", move |cmd| {
    cmd.env("COMPOSE_DOCKER_CLI_BUILD", "1");
    cmd.env("DOCKER_BUILDKIT", "1");
    cmd.arg("-f").arg(&settings.path);
    cmd.arg("--profile").arg(&settings.profile);
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
pub struct Context {
  state: std::sync::Arc<RwLock<State>>,
}

impl Context {
  pub fn new(state: State) -> Self {
    Self {
      state: Arc::new(RwLock::new(state)),
    }
  }

  pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, State> {
    self.state.read().await
  }

  pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, State> {
    self.state.write().await
  }

  pub fn try_write(&self) -> Option<tokio::sync::RwLockWriteGuard<'_, State>> {
    self.state.try_write().ok()
  }

  pub async fn read_owned(&self) -> tokio::sync::OwnedRwLockReadGuard<State> {
    Arc::clone(&self.state).read_owned().await
  }
}
