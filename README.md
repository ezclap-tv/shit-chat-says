# shit-chat-says

Markov chain that can be easily trained on Chatterino logs, and a prompt to generate messages from it.

### Usage

1. Grab some Chatterino logs from your favorite chat(s)
2. Dump them under `/data`
3. `node clean.js`
4. `cargo run --release --bin train`
5. `cargo run --release --bin gen`

The last command opens a prompt, either press enter to get completely random messages,
or some keyword to generate the remainder of the message.