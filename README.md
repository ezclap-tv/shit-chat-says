# shit-chat-says

Markov chain that can be easily trained on Chatterino logs, and a prompt to generate messages from it.

### Usage

#### Training

1. Grab some Chatterino logs from your favorite chat(s)
2. Dump them under `/data`
3. `node clean.js`
4. `cargo run --release --bin train`

Generates `/data/model.yaml`. You should compress this file if you plan to deploy it anywhere, as
it is mostly whitespace.

#### Command-line prompt

Requires `model.yaml` to be present in `/data`

1. `cargo run --release --bin gen`

Either press enter to get completely random messages, or some keyword to generate the remainder of the message.

#### Log collector

1. `cp collector.example.json collector.json`
2. Fill in the config values
  - `channels` tells the collector which channels to join
    - Channel buffer size default is 1KiB, which is ~5-10 messages for every file write syscall.
      You can increase this if you plan to use it for larger channels.
3. `cargo run --release --bin collector`

It will create and write to a `CHANNEL-YYYY-MM-DD.log` file, per-channel, rotating every day. The date is always in UTC.

**NOTE:** Currently, there is no way to incrementally re-train the bot on the collected logs. Progress on this is tracked [here](https://github.com/jprochazk/shit-chat-says/issues/3).

#### Chat bot

Requires `model.yaml` to be present in `/data`

1. `cp chat.example.json chat.json`
2. Fill in the config values
  - `login` is the username you use to login to the bot account
  - `token` [can be generated here](https://twitchapps.com/tmi/)
    - My feeble brain cannot comprehend why twitch requires the login id on top of the token, Twitch devs are simply 10xers,
      so just make sure that the `login` matches the one with which you generated the token
3. `cargo run --release --bin chat`

You can interact with the bot in the channels it joins by `@`ing it, e.g.:

```
Moscowwbish: @my_chat_bot hello
my_chat_bot: hello
```
  