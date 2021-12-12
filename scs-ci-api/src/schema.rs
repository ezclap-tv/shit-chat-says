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
  #[cfg(feature = "cloudflare-hack")]
  #[serde(serialize_with = "cloudflare_hack")]
  pub cloudflare_hack: (),
}

impl CommandOutput {
  pub fn new(output: String, output_kind: OutputKind) -> Self {
    Self {
      output,
      output_kind,
      #[cfg(feature = "cloudflare-hack")]
      cloudflare_hack: (),
    }
  }
}

#[derive(serde::Serialize)]
pub struct CommandResult {
  pub is_success: bool,
  pub status_line: String,

  #[cfg(feature = "cloudflare-hack")]
  #[serde(serialize_with = "cloudflare_hack")]
  pub cloudflare_hack: (),
}

impl CommandResult {
  pub fn new(is_success: bool, status_line: String) -> Self {
    Self {
      is_success,
      status_line,
      #[cfg(feature = "cloudflare-hack")]
      cloudflare_hack: (),
    }
  }
}

#[cfg(feature = "cloudflare-hack")]
fn cloudflare_hack<S>(_: &(), s: S) -> Result<S::Ok, S::Error>
where
  S: serde::Serializer,
{
  s.serialize_str(CLOUDFLARE_PADDING)
}

#[cfg(feature = "cloudflare-hack")]
macro_rules! rep {
  ($t:expr, 4) => {
    concat!($t, $t, $t, $t)
  };
  ($t:expr, 16) => {
    rep!(rep!($t, 4), 4)
  };
  ($t:expr, 64) => {
    rep!(rep!($t, 16), 4)
  };
  ($t:expr, 256) => {
    rep!(rep!($t, 64), 4)
  };
  ($t:expr, 1024) => {
    rep!(rep!($t, 256), 4)
  };
  ($t:expr, 4096) => {
    rep!(rep!($t, 1024), 4)
  };
  ($t:expr, 16384) => {
    rep!(rep!($t, 4096), 4)
  };
}

#[cfg(feature = "cloudflare-hack")]
pub const CLOUDFLARE_PADDING: &str = rep!("\u{200B}", 16384);
