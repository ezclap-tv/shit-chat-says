use anyhow::Result;
use std::env;
use std::ffi::OsStr;
use std::fs;
use walkdir::WalkDir;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "\\logs");
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "\\data\\model.yaml");

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
  println!("Training...");
  let mut chain = markov::Chain::<String>::of_order(2);
  for entry in WalkDir::new(INPUT)
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
  chain.save(OUTPUT)?;
  println!("Done");

  Ok(())
}
