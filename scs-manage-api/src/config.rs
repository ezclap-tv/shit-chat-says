use std::collections::HashSet;

use serde::{de, Deserialize, Deserializer};

const MIN_TOKEN_ENTROPY: f64 = 300.0;

#[derive(Clone, Hash, PartialEq, Eq, Deserialize)]
pub struct AccessToken(#[serde(deserialize_with = "de_from_str")] String);

impl AccessToken {
  pub fn new_not_validated(token: String) -> Self {
    Self(token)
  }
}
impl<'a> std::borrow::Borrow<str> for AccessToken {
  fn borrow(&self) -> &str {
    self.0.borrow()
  }
}

impl std::fmt::Debug for AccessToken {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_tuple("AccessToken")
      .field(&format!("{} ({})", "*".repeat(self.0.len()), self.0.len()))
      .finish()
  }
}

fn de_from_str<'de, D>(deserializer: D) -> Result<String, D::Error>
where
  D: Deserializer<'de>,
{
  let s = String::deserialize(deserializer)?;
  let estimator = cracken::password_entropy::EntropyEstimator::from_files::<std::path::PathBuf>(&[])
    .expect("Failed without performing any IO");
  let entropy = estimator
    .estimate_password_entropy(s.as_bytes())
    .map_err(de::Error::custom)?;
  if entropy.mask_entropy < MIN_TOKEN_ENTROPY {
    return Err(de::Error::custom(format!(
      "A token must have at least {} bits of entropy, but was {}",
      MIN_TOKEN_ENTROPY, entropy.mask_entropy
    )));
  }
  Ok(s)
}

#[derive(Debug, Deserialize)]
pub struct Config {
  pub compose_file: std::path::PathBuf,
  pub project_source_folder: std::path::PathBuf,
  pub access_tokens: HashSet<AccessToken>,
}

impl Config {
  pub fn load<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
    let mut config = serde_json::from_str::<Config>(
      &std::fs::read_to_string(path.as_ref())
        .map_err(|_| anyhow::anyhow!("Could not read the config file {}", path.as_ref().display()))?,
    )?;
    config.compose_file = Self::process_path(&config.compose_file, "Compose file", false)?;
    config.project_source_folder = Self::process_path(&config.project_source_folder, "Source folder", true)?;
    if config.access_tokens.is_empty() {
      log::error!("No access tokens were specified -- the API is going to be be inaccessible. Please provide at least one access token.");
      anyhow::bail!("No access tokens were specified");
    }
    Ok(config)
  }

  fn process_path(
    path: &std::path::Path,
    description: impl AsRef<str>,
    should_be_dir: bool,
  ) -> anyhow::Result<std::path::PathBuf> {
    if !path.exists() {
      log::error!("{} '{}' does not exist", description.as_ref(), path.display());
      anyhow::bail!(format!("{} doesn't exist", description.as_ref()));
    }
    if path.is_dir() && !should_be_dir {
      log::error!("{} '{}' is a directory", description.as_ref(), path.display());
      anyhow::bail!(format!("{} is a directory", description.as_ref()));
    }
    path.canonicalize().map_err(|_| {
      anyhow::anyhow!(format!(
        "{} '{}' is not a valid path (failed to resolve)",
        description.as_ref(),
        path.display()
      ))
    })
  }
}
