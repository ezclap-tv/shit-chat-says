use anyhow::Result;
use std::fs;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "\\data\\data.csv");
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "\\data\\model.yaml");

fn main() -> Result<()> {
  let file = fs::read_to_string(INPUT)?;
  println!("Training on {}", INPUT);
  let mut chain = markov::Chain::<String>::of_order(2);
  for line in file.split('\n').skip(1) {
    let mut parts = line.splitn(4, ',');
    let _channel = parts.next().unwrap();
    let _date = parts.next().unwrap();
    let _user = parts.next().unwrap();
    let message = parts.next().unwrap();
    chain.feed_str(message);
  }
  chain.save(OUTPUT)?;
  println!("Done");
  Ok(())
}
