#!/usr/bin/env bash

if [ -z ${MIGRATIONS_DIR+x} ]; then
	MIGRATIONS_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)
	MIGRATIONS_DIR="$MIGRATIONS_DIR/../scs-db/migrations"
	echo "MIGRATIONS_DIR unset, defaulting to $MIGRATIONS_DIR"
else
	echo "MIGRATIONS_DIR set to $MIGRATIONS_DIR"
fi

db_user=${SCS_DB_USER:=scs}
db_password="${SCS_DB_PASSWORD:=scs}"
db_name="${SCS_DB_NAME:=scs}"
db_port="${SCS_DB_PORT:=5432}"
db_host="${SCS_DB_HOST:=localhost}"

DATABASE_URL="postgres://${db_user}:${db_password}@${db_host:=host.docker.internal}:${db_port}/${db_name}"
sqlx migrate run --source "$MIGRATIONS_DIR" --database-url "$DATABASE_URL"
