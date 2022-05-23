#![feature(iter_intersperse)]
use std::collections::HashMap;
use std::env;
use std::fs;

use anyhow::Result;
use chrono::Utc;
use config::TrainingConfig;
use walkdir::WalkDir;

#[cfg(not(feature = "no-progress"))]
use indicatif::ProgressBar;

mod config;

fn split_line(line: &str) -> Option<(&str, &str)> {
  if !line.trim().is_empty() {
    line.split_once(",")
  } else {
    None
  }
}

#[derive(Default)]
pub struct LogStore {
  channels: HashMap<String, Vec<(String, String)>>,
}

impl LogStore {
  pub fn store(&mut self, channel: &str, filename: String, contents: String) {
    if let Some(store) = self.channels.get_mut(channel) {
      store.push((filename, contents));
    } else {
      self.channels.insert(channel.to_owned(), vec![(filename, contents)]);
    }
  }

  #[inline]
  pub fn has(&self, channel: &str) -> bool {
    self.channels.contains_key(channel)
  }

  pub fn filter<'this>(
    &'this self,
    channel: &'this str,
    config: &'this config::TrainingConfig,
  ) -> impl Iterator<Item = &'this str> {
    config
      .channels
      .get(channel)
      .expect("Attempted to filter logs on a non-existent channel.")
      .iter()
      .map(AsRef::as_ref)
      .chain(std::iter::once(channel))
      .filter_map(move |target_channel| self.channels.get(target_channel))
      .flat_map(|logs| logs.iter().map(|(_, contents)| contents.as_ref()))
  }

  #[inline]
  pub fn all(&self) -> impl Iterator<Item = &'_ str> {
    self
      .channels
      .values()
      .flat_map(|logs| logs.iter().map(|(_, contents)| contents.as_ref()))
  }
}

// NOTE: this uses too much RAM when the input is large (for obvious reasons);
//       should probably rewrite this to collect only the filenames, and then make
//       the log store use some kind of filesize-based cache.
fn collect_logs(store: &mut LogStore, config: &TrainingConfig) {
  #[cfg(not(feature = "no-progress"))]
  let bar =
    ProgressBar::new(!0).with_style(indicatif::ProgressStyle::default_spinner().template("{spinner} {pos} (files)"));

  let all_channels = config
    .channels
    .values()
    .flat_map(|c| c.iter())
    .chain(config.channels.keys())
    .collect::<std::collections::HashSet<_>>();

  for (channel, filename, content) in WalkDir::new(&config.input_directory)
    .into_iter()
    .filter_map(|e| e.ok())
    .filter_map(|entry| {
      entry
        .file_name()
        .to_str()
        .map(|name| (name, config.extract_channel_name(name)))
        .map(|(name, channel)| (channel.to_owned(), name.to_owned(), entry.clone()))
    })
    .filter_map(|(channel, file_name, entry)| {
      if (all_channels.is_empty() || all_channels.contains(&channel)) && config.is_after_date(&file_name) {
        fs::read_to_string(entry.path())
          .ok()
          .map(|contents| (channel, file_name, contents))
      } else {
        None
      }
    })
  {
    #[cfg(not(feature = "no-progress"))]
    bar.inc(1);
    store.store(&channel, filename.to_owned(), content);
  }

  #[cfg(not(feature = "no-progress"))]
  bar.finish_at_current_pos();
}

fn train<'a>(chain: &mut chain::Chain<2>, authored_mode: bool, logs: impl Iterator<Item = &'a str>) {
  #[cfg(not(feature = "no-progress"))]
  let bar =
    ProgressBar::new(!0).with_style(indicatif::ProgressStyle::default_spinner().template("{spinner} {pos} (files)"));

  for log in logs {
    #[cfg(not(feature = "no-progress"))]
    bar.inc(1);
    for (user, message) in log.split('\n').filter_map(split_line) {
      if authored_mode {
        chain.feed_str(&format!("{}: {}", user, message.trim()));
      } else {
        chain.feed_str(message.trim());
      }
    }
  }

  #[cfg(not(feature = "no-progress"))]
  bar.finish_at_current_pos();
}

fn save_model<const ORDER: usize>(
  chain: &chain::Chain<ORDER>,
  name: &str,
  output_path: &std::path::Path,
  save_timestamped_checkpoint: bool,
) -> anyhow::Result<()> {
  if save_timestamped_checkpoint {
    chain.save(&output_path.join(format!("{}-{}.chain", name, Utc::today().format("%F"))))?;
  }
  chain.save(&output_path.join(format!("{}.chain", name)))?;
  Ok(())
}

fn main() -> Result<()> {
  if env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "INFO");
  }
  scs_sentry::from_env!();
  env_logger::init();

  let config = if let Some(path) = env::args().nth(1) {
    config::TrainingConfig::load(&std::path::PathBuf::from(path))?
  } else {
    config::TrainingConfig::default()
  };
  log::info!("Loaded config {:?}", config);

  let mut store = LogStore::default();

  log::info!("Collecting logs...");
  collect_logs(&mut store, &config);

  let mut base_chain = if let Some(path) = &config.model_to_fine_tune {
    log::info!("Loading a previous model for fine-tuning...");
    chain::Chain::<2>::load(path)?
  } else {
    chain::of_order!(2)
  };

  if config.channels.is_empty() {
    log::info!("Training a model on all data...");
    train(&mut base_chain, config.authored_mode, store.all());

    log::info!("Saving the model...");
    save_model(
      &base_chain,
      "model",
      &config.output_directory,
      config.save_timestamped_checkpoint,
    )?;
    return Ok(());
  }

  log::info!("Training per-channel models");
  for channel in config.channels.keys() {
    log::info!("=> Training for {}", channel);

    let mut chain = base_chain.clone().with_metadata(format!(
      "{{ channels: {}; order: {} }}",
      std::iter::once(channel)
        .chain(config.channels[channel].iter())
        .map(|s| s.as_ref())
        .intersperse(",")
        .collect::<String>(),
      base_chain.order()
    ));
    train(&mut chain, config.authored_mode, store.filter(channel, &config));
    log::info!("=> Saving {}.chain...", channel);
    save_model(
      &chain,
      channel,
      &config.output_directory,
      config.save_timestamped_checkpoint,
    )?;
  }

  log::info!("Done");

  Ok(())
}
