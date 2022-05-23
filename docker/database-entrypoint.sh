#!/bin/bash
set -ex

echo "SQLX: WAITING FOR THE DATABASE..."
wait-for-it "${DATABASE_HOST}:${DATABASE_PORT}" -t 120

echo "SQLX: RUNNING MIGRATIONS"

# database create should be able to fail on consequen runs
set +ex
/binaries/sqlx database create
set -ex

/binaries/sqlx migrate run

echo "SQLX: DONE"
