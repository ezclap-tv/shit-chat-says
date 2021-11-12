use anyhow::Result;

const MODEL_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "\\data\\model.yaml");

fn main() -> Result<()> {
  println!("Loading model from {}...", MODEL_DIR);
  let chain = markov::Chain::<String>::load(MODEL_DIR)?;
  let mut rl = rustyline::Editor::<()>::new();
  while let Ok(line) = rl.readline(">> ") {
    let line = line.as_str().trim();
    let generated = if line.is_empty() {
      chain.generate_str()
    } else {
      rl.add_history_entry(line);
      chain.generate_str_from_token(line)
    };
    println!("{}", generated);
  }
  Ok(())
}
