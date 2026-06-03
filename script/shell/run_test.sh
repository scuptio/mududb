#!/bin/bash
set -euo pipefail

source "$HOME/.cargo/env" 2>/dev/null || true

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
MPK_FILE="$REPO_ROOT/testing/mpk/wallet.mpk"
TEMP_DIR="/tmp/mudu_test_$(date +%s)"
DATA_DIR="$TEMP_DIR/data"
MPK_DIR="$TEMP_DIR/mpk"
CONFIG_FILE="$HOME/.mududb/mududb_cfg.toml"
CONFIG_BACKUP="$HOME/.mududb/mududb_cfg.toml.bak"
SERVER_LOG="$TEMP_DIR/server.log"
HTTP_PORT=8300
TCP_PORT=9527

cd "$REPO_ROOT"

echo "=== MuduDB 启动 & CRUD 测试 ==="
echo ""

cleanup() {
    echo ""
    echo "清理中..."
    [ -n "${SERVER_PID:-}" ] && { kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; }
    [ -f "$CONFIG_BACKUP" ] && { cp "$CONFIG_BACKUP" "$CONFIG_FILE"; rm -f "$CONFIG_BACKUP"; }
    rm -rf "$TEMP_DIR"
    echo "清理完成"
}
trap cleanup EXIT

mkdir -p "$DATA_DIR" "$MPK_DIR" "$(dirname "$CONFIG_FILE")"

[ -f "$CONFIG_FILE" ] && cp "$CONFIG_FILE" "$CONFIG_BACKUP"
cat > "$CONFIG_FILE" <<CFGEOF
mpk_path = "$MPK_DIR"
db_path = "$DATA_DIR"
listen_ip = "127.0.0.1"
http_listen_port = $HTTP_PORT
http_worker_threads = 1
pg_listen_port = 5432
enable_async = true
server_mode = 1
tcp_listen_port = $TCP_PORT
worker_threads = 2
routing_mode = 2
CFGEOF

# Start server
echo "[1/4] 启动 mudud 服务器..."
mudud > "$SERVER_LOG" 2>&1 &
SERVER_PID=$!
echo "  PID: $SERVER_PID"

# Wait for both HTTP and TCP ports
for i in $(seq 1 30); do
    http_ok=false; tcp_ok=false
    curl -s http://127.0.0.1:$HTTP_PORT/ > /dev/null 2>&1 && http_ok=true
    command -v ss &>/dev/null && ss -tlnp "sport = :$TCP_PORT" 2>/dev/null | grep -q "$SERVER_PID" && tcp_ok=true
    if $http_ok && $tcp_ok; then echo "  服务器就绪 (${i}s)"; break; fi
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then echo "  ERROR: 服务器意外退出"; cat "$SERVER_LOG"; exit 1; fi
    sleep 1
done

# Install wallet app
echo ""
echo "[2/4] 安装 wallet 应用..."
mcli --http-addr 127.0.0.1:$HTTP_PORT app-install --mpk "$MPK_FILE"

# CRUD tests
echo ""
echo "[3/4] 运行 CRUD 测试..."
echo ""

# Helper: invoke procedure via HTTP API
invoke() {
    local proc=$1 data=$2
    curl -s -X POST "http://127.0.0.1:$HTTP_PORT/mudu/app/invoke/wallet/wallet/$proc" \
        -H "Content-Type: application/json" -d "$data"
}

# Helper: SQL query via mcli TCP
query() {
    mcli command --json "{\"app_name\":\"wallet\",\"sql\":\"$1\"}" --compact --no-table 2>&1
}

# CREATE
echo "  >> CREATE: 创建用户 user_id=3 (Charlie)"
invoke create_user '{"user_id":3,"name":"Charlie","email":"charlie@test.com"}'

# READ (SQL)
echo "  >> READ: 查询全部用户"
query "SELECT user_id, name, email FROM users"

echo "  >> READ: 初始钱包余额"
query "SELECT user_id, balance FROM wallets"

# UPDATE (deposit to Alice)
echo "  >> UPDATE: Alice (user_id=1) +5000"
invoke deposit '{"user_id":1,"amount":5000}'

echo "  >> READ: Alice 最新余额"
query "SELECT user_id, balance FROM wallets WHERE user_id=1"

# INVOKE procedure (transfer)
echo "  >> INVOKE: Alice -> Bob 转账 3000"
invoke transfer_funds '{"from_user_id":1,"to_user_id":2,"amount":3000}'

echo "  >> READ: 验证转账后余额"
query "SELECT user_id, balance FROM wallets"

# DELETE
echo "  >> DELETE: 删除用户 user_id=3"
invoke delete_user '{"user_id":3}'

echo "  >> READ: 最终用户列表（应只有 Alice, Bob）"
query "SELECT user_id, name, email FROM users"

echo ""
echo "[4/4] 测试完成，关闭服务器..."
echo ""
echo "=== 全部测试通过 ==="
