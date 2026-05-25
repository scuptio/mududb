#!/bin/bash
# Build MuduDB, install binaries, and start the server.
# Usage: ./build_and_start.sh [--clean]
#   --clean  Remove existing data directory before starting
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
DATA="/tmp/mudu_manual_test/data"
LOG="/tmp/mudu_manual_test/server.log"
MPK_DIR="/tmp/mudu_manual_test/mpk"

CLEAN=false
[[ "${1:-}" == "--clean" ]] && CLEAN=true

echo "[1/3] Building..."
cargo build --release -p mudud -p mudu_cli 2>&1 | tail -3

echo "[2/3] Installing binaries..."
pkill -9 mudud 2>/dev/null || true
sleep 1
cp "$ROOT/target/release/mudud" ~/.cargo/bin/mudud
cp "$ROOT/target/release/mcli" ~/.cargo/bin/mcli

echo "[3/3] Starting server..."
mkdir -p "$DATA" "$MPK_DIR"
if $CLEAN; then
    rm -rf "$DATA"/*
fi

# Write config
cat > ~/.mudu/mududb_cfg.toml <<EOF
mpk_path = "$MPK_DIR"
db_path = "$DATA"
listen_ip = "127.0.0.1"
http_listen_port = 8300
http_worker_threads = 1
pg_listen_port = 5432
enable_async = true
server_mode = 1
tcp_listen_port = 9527
io_uring_worker_threads = 2
routing_mode = 2
EOF

RUST_LOG=mudu_runtime=info mudud > "$LOG" 2>&1 &
SERVER_PID=$!
echo "mudud PID: $SERVER_PID, log: $LOG"

# Wait for ready
for i in $(seq 1 30); do
    curl -s http://127.0.0.1:8300/ > /dev/null 2>&1 && echo "Server ready (${i}s)" && exit 0
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
        echo "Server failed to start. Log:"
        cat "$LOG"
        exit 1
    fi
    sleep 1
done
echo "Timeout waiting for server. Log:"
cat "$LOG"
exit 1
