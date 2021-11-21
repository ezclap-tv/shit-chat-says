use anyhow::Result;
use chrono::Utc;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

fn split_line(line: &str) -> Option<(&str, &str)> {
  if !line.trim().is_empty() {
    let mut parts = line.splitn(2, ',');
    let user = parts.next();
    let message = parts.next();
    if let Some(user) = user {
      if let Some(message) = message {
        return Some((user, message));
      }
    }
  }

  None
}

fn main() -> Result<()> {
  let input = std::env::var("SCS_INPUT_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|_| PathBuf::from(CARGO_MANIFEST_DIR).join("logs"));
  let mut output = std::env::var("SCS_MODEL_PATH")
    .map(PathBuf::from)
    .unwrap_or_else(|_| PathBuf::from(CARGO_MANIFEST_DIR).join("models").join("model.yaml"));

  println!("Training...");
  let mut chain = markov::Chain::<String>::of_order(2);
  for entry in WalkDir::new(input)
    .into_iter()
    .filter_map(|e| e.ok())
    .filter(|entry| entry.path().extension().and_then(OsStr::to_str) == Some("log"))
    .filter_map(|entry| fs::read_to_string(entry.path()).ok())
  {
    for (_, message) in entry.split('\n').filter_map(split_line) {
      chain.feed_str(message);
    }
  }

  println!("Saving model...");

  if output.is_dir() {
    chain.save(&output.join(format!("model-{}.yaml", Utc::today().format("%F"))))?;
    output = output.join("model.yaml");
  }

  chain.save(output)?;
  println!("Done");

  Ok(())
}
