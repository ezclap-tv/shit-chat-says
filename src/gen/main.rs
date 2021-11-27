use std::path::PathBuf;

use anyhow::Result;

const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

fn main() -> Result<()> {
  let model_dir = std::env::var("SCS_MODEL_PATH")
    .map(PathBuf::from)
    .unwrap_or_else(|_| PathBuf::from(CARGO_MANIFEST_DIR).join("models").join("model.yaml"));

  println!("Loading model from {}...", model_dir.display());
  let chain = chain::load_chain_of_any_supported_order(model_dir)?;
  let mut rl = rustyline::Editor::<()>::new();
  while let Ok(line) = rl.readline(">> ") {
    let line = line.as_str().trim();
    let generated = if line.is_empty() {
      chain::sample(&chain, "", 16)
    } else {
      rl.add_history_entry(line);
      chain::sample(&chain, line, 16)
    };
    println!("{}", generated);
  }
  Ok(())
}
