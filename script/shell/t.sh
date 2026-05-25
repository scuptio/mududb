#!/bin/bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DATA="/tmp/mudu_manual_test/data"
LOG="/tmp/mudu_manual_test/server.log"
BIN="$REPO_ROOT/target/release/mudud"

pkill -9 mudud 2>/dev/null || true
sleep 1
cp "$BIN" ~/.cargo/bin/mudud
rm -rf "$DATA"/*
mkdir -p "$DATA"
RUST_LOG=mudu_runtime=info mudud > "$LOG" 2>&1 &
echo "mudud PID: $!, log: $LOG"
sleep 5
cat "$LOG"
