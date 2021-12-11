# shit-chat-says

Markov chain that can be easily trained on Chatterino logs, and a prompt to generate messages from it.

### Usage

#### With Docker

Requires `docker-compose` v1.28+

Not required, but recommended: Use BuildKit

```bash
$ # bash
$ COMPOSE_DOCKER_CLI_BUILD=1 DOCKER_BUILDKIT=1 <command>
```

```ps1
PS > # powershell
PS > $env:COMPOSE_DOCKER_CLI_BUILD=1; $env:DOCKER_BUILDKIT=1; <command>
```

1. Build the base image

```bash
$ docker-compose -f docker/docker-compose.yml build
```

2. Run the binary you want (todo: start all services independently instead)

```bash
$ docker-compose -f docker/docker-compose.yml run --rm collector
```

3. To run multiple services at the same time, use the command below (add `-d` to run it in the background)

```bash
$ docker-compose -f docker/docker-compose.yml up
```

#### Without Docker

##### Log collector

1. `cp config/collector.example.json config/collector.json`
2. Fill in the config values

- `channels` tells the collector which channels to join
  - Channel buffer size default is 1KiB, which is ~5-10 messages for every file write syscall.
    You can increase this if you plan to use it for larger channels.
- `output_directory` tells the collector where to write logs
- (optional) `credentials` with which the bot should join the chat. The collector never sends any messages, the reason this exists is that anonymous chatters are rate limited and deprioritized, and logging in removes those limitations
  - `login` is your channel name (in lowercase)
  - `token` [can be generated here](https://twitchapps.com/tmi/)
    - Ensure that `login` matches the one used to generate the `token`

3. `cargo run --release --bin collector`

It will write to a `CHANNEL-YYYY-MM-DD.log` file, per-channel, rotating every day. The date is always in UTC.

##### Training

1. Grab some Chatterino logs from your favorite chat(s)
2. Dump them under `/data`
3. `node clean.js`
4. (optional) `cp config/train.example.json config/train.json` + fill in values
5. `cargo run --release --bin train`

##### Command-line prompt

Requires a trained model to be available.

- `cargo run --release --bin gen`

Either press enter to get completely random messages, or a word to generate the remainder of the message.

##### Chat bot

Requires a trained model to be available.

1. `cp config/chat.example.json config/chat.json`
2. Fill in the config values

- `login` is the username you use to login to the bot account
- `token` [can be generated here](https://twitchapps.com/tmi/)
  - Ensure that `login` matches the one used to generate the `token`
- `channels` is an array of channel names to collect logs in
- (optional) `reply_probability` is the likelihood (from 0 to 1) that the bot will respond to a message
- (optional) `reply_timeout` is the minimum interval (in seconds) between the bot's responses
- (optional) `reply_after_messages` is the number of messages the bot must see before it responds to a message
- (optional) `reply_blocklist` is a list of usernames to ignore (e.g. `streamelements`)
- (optional) `model_path` is the path to the model it should use to generate messages

3. `cargo run --release --bin chat`

You can interact with the bot in the channels it joins by `@`ing it, e.g.:

```
Moscowwbish: @my_chat_bot hello
my_chat_bot: hello
```

It accepts up to two words as input, and everything else from your message is discarded.

```
Moscowwbish: @my_chat_bot hello bot
my_chat_bot: hello bot peepoHey
```

You can also use the `$` prefix to query phrase metadata:

```
Moscowwbish: $my_chat_bot test
my_chat_bot: model.chain (version: {...}, metadata: {
-> Word: `test`
-> word_id: SymbolU32 { value: 12268 }
-> keys:
  -> key: [None, Some("test")]
  -> edge_count: 17
  -> top(5) edges:
    -> <None>: 241
    -> test: 14
    -> the: 3
    -> this: 2
    -> it: 2
})
