if [ -z ${SEED_DIR+x} ]; then
	SEED_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)
	SEED_DIR="$SEED_DIR/../scs-db/seed"
	echo "SEED_DIR unset, defaulting to $SEED_DIR"
else
	echo "SEED_DIR set to $SEED_DIR"
fi

db_user=${SCS_DB_USER:=scs}
db_password="${SCS_DB_PASSWORD:=scs}"
db_name="${SCS_DB_NAME:=scs}"
db_port="${SCS_DB_PORT:=5432}"
db_host="${SCS_DB_HOST:=localhost}"

PGPASSWORD="${db_password}" psql -h "${db_host}" -U "${db_user}" -p "${db_port}" \
	--file "${SEED_DIR}/base.sql"
