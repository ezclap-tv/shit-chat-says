#!/bin/bash


# The lines below display the cron output in docker logs. Code taken from https://stackoverflow.com/a/63346931.

# Create custom stdout and stderr named pipes
mkfifo /tmp/stdout /tmp/stderr
chmod 0666 /tmp/stdout /tmp/stderr

# Have the main Docker process tail the files to produce stdout and stderr 
# for the main process that Docker will actually show in docker logs.
tail -f /tmp/stdout &
tail -f /tmp/stderr >&2 &

echo "Saving $TRAIN_CONFIG and \$SCS_DATABASE_URL to /binaries/env"
echo "export TRAIN_CONFIG=$TRAIN_CONFIG
export SCS_DATABASE_URL=$SCS_DATABASE_URL" >> /binaries/env
# cat /binaries/env

# Run cron
cron -f
