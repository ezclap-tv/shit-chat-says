# shit-chat-says

Markov chain that can be easily trained on Chatterino logs, and a prompt to generate messages from it.

### Usage

#### Docker

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

#### Log collector

1. `cp config/collector.example.json config/collector.json`
2. Fill in the config values

- `channels` tells the collector which channels to join
  - Channel buffer size default is 1KiB, which is ~5-10 messages for every file write syscall.
    You can increase this if you plan to use it for larger channels.

3. `cargo run --release --bin collector`

It will write to a `CHANNEL-YYYY-MM-DD.log` file, per-channel, rotating every day. The date is always in UTC.

#### Training

1. Grab some Chatterino logs from your favorite chat(s)
2. Dump them under `/data`
3. `node clean.js`
4. `cargo run --release --bin train`

Generates `/models/model.yaml`

#### Command-line prompt

Requires `model.yaml` to be present in `/models`

1. `cargo run --release --bin gen`

Either press enter to get completely random messages, or a word to generate the remainder of the message.

#### Chat bot

Requires `model.yaml` to be present in `/models`

1. `cp config/chat.example.json config/chat.json`
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
