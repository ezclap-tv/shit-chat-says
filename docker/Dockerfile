# syntax = docker/dockerfile:experimental
# Lastest nightly (2021-11-19), cc https://hub.docker.com/layers/rustlang/rust/nightly-slim/images/sha256-6f951974ec2e57911a55fbd44ae85ba474084b0820bc060aeab3decbbd3fc582
FROM rustlang/rust@sha256:6f951974ec2e57911a55fbd44ae85ba474084b0820bc060aeab3decbbd3fc582 as builder

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
    RUSTFLAGS=$RUSTFLAGS cargo build --all --release && \
    cp $HOME/app/target/release/gen       $HOME/binaries && \
    cp $HOME/app/target/release/chat      $HOME/binaries && \
    cp $HOME/app/target/release/train     $HOME/binaries && \
    cp $HOME/app/target/release/ingest    $HOME/binaries && \
    cp $HOME/app/target/release/collector $HOME/binaries


# Use a slim image for the runtime image
FROM debian:buster-slim as runtime

RUN apt update && \
    apt install -y ca-certificates && \
    update-ca-certificates  && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /home/rust/binaries /binaries

# Create a new stage for the cron-executed train
FROM debian:buster-slim as trainer

RUN apt update && \ 
    apt -y install cron && \ 
    rm -rf /var/lib/apt/lists/*

# Copy the binaries
COPY --from=builder /home/rust/binaries /binaries

# Setup cron
COPY ./docker/train.sh  /binaries/train.sh
RUN chmod +x            /binaries/train.sh

COPY ./docker/train.cron /etc/cron.d/train
RUN chmod +x             /etc/cron.d/train

COPY ./docker/train-entrypoint.sh /train-entrypoint.sh
RUN chmod +x                      /train-entrypoint.sh

RUN crontab /etc/cron.d/train
RUN touch /var/log/cron.log

# Run cron on container startup

# The lines below display the cron output in docker logs. Code taken from https://stackoverflow.com/a/54250753.
ENTRYPOINT [ "/train-entrypoint.sh" ]
