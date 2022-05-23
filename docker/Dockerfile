# syntax = docker/dockerfile:experimental
# Lastest nightly (2022-02-14), cc https://hub.docker.com/layers/rustlang/rust/nightly-slim/images/sha256-2b16c0cc1b5c4860efd04ae3d8c17a3fb19693e5b9d3b10c6ded26e7cb69deb3
FROM rustlang/rust@sha256:2b16c0cc1b5c4860efd04ae3d8c17a3fb19693e5b9d3b10c6ded26e7cb69deb3 as builder

RUN useradd -m rust

ENV HOME /home/rust

RUN mkdir -p $HOME/app
RUN mkdir -p $HOME/app/target
RUN mkdir -p $HOME/binaries
RUN chown rust:rust -R $HOME


# Copy the code and Cargo metamodels files into the image
COPY ./Cargo.lock            $HOME/app/Cargo.lock
COPY ./Cargo.toml            $HOME/app/Cargo.toml
COPY ./src                   $HOME/app/src
COPY ./scs-db                $HOME/app/scs-db
COPY ./scs-chain             $HOME/app/scs-chain
COPY ./scs-sentry            $HOME/app/scs-sentry
COPY ./scs-user-api          $HOME/app/scs-user-api
COPY ./scs-manage-api        $HOME/app/scs-manage-api


# Make /app the CWD
WORKDIR /home/rust/app

# Install OpenSSL
RUN apt-get update && apt-get install -y pkg-config libssl-dev g++

# By default, build for the current cpu
ENV RUSTFLAGS "-C target-cpu=native"
RUN --mount=type=cache,target=/home/rust/.cargo/git \
  --mount=type=cache,target=/home/rust/.cargo/registry \
  --mount=type=cache,sharing=private,target=/home/rust/app/target \
  cargo install sqlx-cli && \
  RUSTFLAGS=$RUSTFLAGS cargo build --all --release && \
  cp $HOME/app/target/release/gen       $HOME/binaries && \
  cp $HOME/app/target/release/chat      $HOME/binaries && \
  cp $HOME/app/target/release/train     $HOME/binaries && \
  cp $HOME/app/target/release/ingest    $HOME/binaries && \
  cp $HOME/app/target/release/transfer  $HOME/binaries && \
  cp $HOME/app/target/release/collector $HOME/binaries && \
  cp /usr/local/cargo/bin/sqlx          $HOME/binaries


# Use a slim image for the runtime image
FROM debian:buster-slim as runtime

RUN apt update && \
  apt install -y ca-certificates && \
  update-ca-certificates  && \
  rm -rf /var/lib/apt/lists/*

COPY --from=builder /home/rust/binaries /binaries

# Create a new stage for the cron-executed tasks
FROM debian:buster-slim as tasks

RUN apt update && \ 
  apt -y install cron && \ 
  rm -rf /var/lib/apt/lists/*

# Copy the binaries
COPY --from=builder /home/rust/binaries /binaries

# Setup cron
COPY ./docker/tasks     /binaries/tasks
RUN chmod +x            /binaries/tasks/*.sh

COPY ./docker/tasks.cron /etc/cron.d/tasks
RUN chmod +x             /etc/cron.d/tasks

COPY ./docker/tasks-entrypoint.sh /tasks-entrypoint.sh
RUN chmod +x                      /tasks-entrypoint.sh

RUN crontab /etc/cron.d/tasks
RUN touch /var/log/cron.log

# Run cron on container startup
ENTRYPOINT [ "/tasks-entrypoint.sh" ]

# Create a new stage for the database
FROM debian:buster-slim as database
COPY --from=builder /home/rust/binaries/sqlx /binaries/sqlx
COPY --from=builder /home/rust/app/scs-db    /scs-db

RUN apt update && \ 
  apt -y install wait-for-it pkg-config libssl-dev && \ 
  rm -rf /var/lib/apt/lists/*

ARG DATABASE_URL
ARG DATABASE_HOST
ARG DATABASE_PORT
ENV DATABASE_URL $DATABASE_URL
ENV DATABASE_HOST $DATABASE_HOST
ENV DATABASE_PORT $DATABASE_PORT

WORKDIR /scs-db

COPY ./docker/database-entrypoint.sh /database-entrypoint.sh
RUN chmod +x                         /database-entrypoint.sh

ENTRYPOINT [ "/database-entrypoint.sh" ]
