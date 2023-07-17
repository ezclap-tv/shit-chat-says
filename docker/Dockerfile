# syntax = docker/dockerfile:experimental
# Lastest nightly (2023-07-01), cc https://hub.docker.com/layers/rustlang/rust/nightly-slim/images/sha256-d96496b0f59cbb89e6dd27c8c04153870cbed6cef803165bd21a9f9352384616?context=explore
FROM rustlang/rust@sha256:d96496b0f59cbb89e6dd27c8c04153870cbed6cef803165bd21a9f9352384616 as start

ENV RUSTFLAGS "-C target-cpu=native"
ENV HOME /home/rust

RUN useradd -m rust
RUN mkdir -p $HOME/app
RUN mkdir -p $HOME/app/target
RUN mkdir -p $HOME/binaries
RUN chown rust:rust -R $HOME
WORKDIR $HOME/app

# Install OpenSSL
RUN apt-get update && apt-get install -y pkg-config libssl-dev g++

# Install cargo-chef (tool for pre-building dependencies) and sqlx-cli
RUN cargo install cargo-chef && \
  cargo install sqlx-cli --no-default-features --features rustls,postgres

# Figure out what dependencies we need to build
FROM start AS planner
COPY ./Cargo.lock            $HOME/app/Cargo.lock
COPY ./Cargo.toml            $HOME/app/Cargo.toml
COPY ./src                   $HOME/app/src
COPY ./scs-db                $HOME/app/scs-db
COPY ./scs-chain             $HOME/app/scs-chain
COPY ./scs-ingest            $HOME/app/scs-ingest
COPY ./scs-user-api          $HOME/app/scs-user-api
COPY ./scs-manage-api        $HOME/app/scs-manage-api
RUN cargo chef prepare --recipe-path "$HOME/app/recipe.json"

# The build happens here
FROM start AS builder
COPY --from=planner $HOME/app/recipe.json $HOME/app/recipe.json

# Pre-build the dependencies. This is the layer that gets cached as long as dependecies stay the same.
RUN cargo chef cook --workspace --release --recipe-path "$HOME/app/recipe.json"
# Build the app binaries
COPY ./Cargo.lock            $HOME/app/Cargo.lock
COPY ./Cargo.toml            $HOME/app/Cargo.toml
COPY ./src                   $HOME/app/src
COPY ./scs-db                $HOME/app/scs-db
COPY ./scs-chain             $HOME/app/scs-chain
COPY ./scs-ingest            $HOME/app/scs-ingest
COPY ./scs-user-api          $HOME/app/scs-user-api
COPY ./scs-manage-api        $HOME/app/scs-manage-api
RUN cargo build --workspace --release && \
  cp $HOME/app/target/release/gen       $HOME/binaries && \
  cp $HOME/app/target/release/chat      $HOME/binaries && \
  cp $HOME/app/target/release/train     $HOME/binaries && \
  cp $HOME/app/target/release/ingest    $HOME/binaries && \
  cp $HOME/app/target/release/collector $HOME/binaries && \
  cp $HOME/app/target/release/scs-user-api $HOME/binaries/scs-user-api

# Use a slim image for the runtime image
FROM debian:bullseye-slim as runtime

RUN apt update && \
  apt install -y ca-certificates && \
  update-ca-certificates  && \
  rm -rf /var/lib/apt/lists/*

COPY --from=builder /home/rust/binaries /binaries

# Create a new stage for the cron-executed train
FROM debian:bullseye-slim as trainer

RUN apt update && \ 
  apt -y install cron && \ 
  rm -rf /var/lib/apt/lists/*

# Copy the binaries
COPY --from=builder /home/rust/binaries /binaries

# Setup cron
COPY ./docker/scripts/train.sh  /binaries/train.sh
RUN chmod +x                    /binaries/train.sh

COPY ./docker/scripts/train.cron /etc/cron.d/train
RUN chmod +x                     /etc/cron.d/train

COPY ./docker/scripts/train-entrypoint.sh /train-entrypoint.sh
RUN chmod +x                              /train-entrypoint.sh

RUN crontab /etc/cron.d/train
RUN touch /var/log/cron.log

# Run cron on container startup
ENTRYPOINT [ "/train-entrypoint.sh" ]

# Migrations stage
FROM --platform=$TARGETPLATFORM postgres:15 as migrations
COPY --from=builder /usr/local/cargo/bin/sqlx /bin

COPY ./scs-db/seed       /seed
COPY ./scs-db/migrations /migrations
COPY ./docker/scripts    /scripts

RUN chmod +x /scripts/migrate.sh
RUN chmod +x /scripts/seed.sh

ENV SEED_DIR /seed
ENV MIGRATIONS_DIR /migrations
CMD ["sh", "-c", "/scripts/migrate.sh && /scripts/seed.sh"]
