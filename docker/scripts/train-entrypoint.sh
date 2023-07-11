#!/bin/bash
# Taken from https://stackoverflow.com/a/63346931.

# Create custom stdout and stderr named pipes
mkfifo /tmp/stdout /tmp/stderr
chmod 0666 /tmp/stdout /tmp/stderr

# Have the main Docker process tail the files to produce stdout and stderr 
# for the main process that Docker will actually show in docker logs.
tail -f /tmp/stdout &
tail -f /tmp/stderr >&2 &

echo "Saving $TRAIN_CONFIG to /binaries/train-config"
echo "export TRAIN_CONFIG=$TRAIN_CONFIG" >> /binaries/train-config
cat /binaries/train-config

# Run cron
cron -f
