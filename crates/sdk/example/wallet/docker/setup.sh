#!/bin/bash
# copy paste from
# https://raw.githubusercontent.com/sfackler/rust-postgres/refs/heads/master/docker/sql_setup.sh
set -e


cat >> "$PGDATA/postgresql.conf" <<-EOCONF
port = 5432
ssl = off
EOCONF

cat > "$PGDATA/pg_hba.conf" <<-EOCONF
# TYPE  DATABASE        USER            ADDRESS                 METHOD
# IPv4 local connections:
host    all             postgres        0.0.0.0/0            trust
# IPv6 local connections:
host    all             postgres        ::0/0                trust
# Unix socket connections:
local   all             postgres                             trust
EOCONF


