## scs-chain

This library implements a resource efficient string-oriented markov chain based on the [markov](https://docs.rs/markov/latest/markov/) crate.

In contrast with `markov`, `scs-chain` uses interning, a fixed-order internal representation of nodes and edges, and a custom binary format to make training, executing, saving, and loading chains faster and more memory efficient. Chains up to the 6th order are supported by default.

## Sample Usage

### Training

```rust
use chain::{Chain, of_order};

// Create a chain of the 2nd order. The metadata builder method allows you to provide an arbitrary string to be serialized with the chain when saving (in this case, a JSON object).
let mut chain = chain::of_order!(2).with_metadata(r#"{ "data": "twitch-logs-2021-12", "order": 2 }"#);

// Train it on some text where every line is a sentence
for line in load_logs().lines() {
    chain.feed_str(line.trim());
}

// Generate some output.
// The order of the chain is known, so direct instance methods like generate() can be used:
println!("{}", chain.generate());

// Save the chain to a binary .chain file
chain.save("model.chain".into());
```

### Loading a trained model and generating text

```rust
use chain::load_chain_of_any_supported_order;

// Load a chain of any of the orders supported by the library.
// Since the `ORDER` generic is constant, this method return a trait object in order
// to support loading chains of any order. If you wish to load a chain of a specific order, use chain::Chain::<ORDER>::load() instead.
let mut chain = load_chain_of_any_supported_order("model.chain").unwrap();

// Print the model metadata
println!("order: {}", chain.order());
println!("model.chain metadata: `{}`", chain.model_meta_data());

// Generate some output with different input configurations
println!("{}", chain.generate_text()); // the chain will be seeded with a random word (None, None)
println!("{}", chain.generate_text_from_token("the")); // the chain will be seeded with (None, "the")
println!("{}", chain.try_generate_text_from_token_sequence(&["an", "apple"]).expect("Number of words was != chain.order()")); // the chain will be seeded with  ("an", "apple")


// However, depending on the data, the chain may generate no output or the exact same output as the input.
// In order to get more interesting results, one of the sampling functions can be used:
let max_samples = 4;
println!("{}", chain::sample(&chain, "", max_samples));
println!("{}", chain::sample(&chain, "the", max_samples));
println!("{}", chain::sample_seq(&model, &["an", "apple"], max_samples));
```
