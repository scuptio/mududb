#!/bin/bash
set -uo pipefail

source "$HOME/.cargo/env" 2>/dev/null || true

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
MPK_FILE="$REPO_ROOT/testing/mpk/wallet.mpk"
TEMP_DIR="/tmp/mudu_debug_$(date +%s)"
DATA_DIR="$TEMP_DIR/data"
MPK_DIR="$TEMP_DIR/mpk"
CONFIG_FILE="$TEMP_DIR/mudud.cfg"
SERVER_LOG="$TEMP_DIR/server.log"
HTTP_PORT=8300
TCP_PORT=9527

cd "$TEMP_DIR"

pass=0
fail=0

check() {
    local desc=$1
    shift
    echo -n "  [$desc] "
    output=$("$@" 2>&1)
    rc=$?
    if [ $rc -eq 0 ]; then
        echo "OK"
        ((pass++))
    else
        echo "FAIL (rc=$rc)"
        echo "    output: $output"
        ((fail++))
    fi
    return $rc
}

check_contains() {
    local desc=$1 pattern=$2
    shift 2
    echo -n "  [$desc] "
    output=$("$@" 2>&1)
    if echo "$output" | grep -q "$pattern"; then
        echo "OK"
        ((pass++))
    else
        echo "FAIL (expected '$pattern')"
        echo "    output: $output"
        ((fail++))
    fi
}

cleanup() {
    echo ""
    echo "=== 清理 ==="
    [ -n "${SERVER_PID:-}" ] && { kill -9 "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; }
    echo "临时目录: $TEMP_DIR (保留用于分析)"
    echo ""
    echo "=== 结果: $pass passed, $fail failed ==="
    echo "日志: $SERVER_LOG"
}
trap cleanup EXIT

echo "=== MuduDB 调试测试 ==="
echo "临时目录: $TEMP_DIR"
echo ""

# Setup
mkdir -p "$DATA_DIR" "$MPK_DIR"

# Step 1: Write config
echo "[1] 写入配置"
cat > "$CONFIG_FILE" <<CFGEOF
mpk_path = "$MPK_DIR"
db_path = "$DATA_DIR"
listen_ip = "127.0.0.1"
http_listen_port = $HTTP_PORT
http_worker_threads = 1
pg_listen_port = 5432
enable_async = true
server_mode = "IOUring"
tcp_listen_port = $TCP_PORT
worker_threads = 2
routing_mode = "RemoteHash"
CFGEOF
echo "  配置文件: $CONFIG_FILE"
echo "  db_path: $DATA_DIR"
echo "  mpk_path: $MPK_DIR"
echo ""

# Step 2: Start server
echo "[2] 启动 mudud 服务器"
RUST_LOG=mudu_runtime=info,mudu_kernel=info mudud serve > "$SERVER_LOG" 2>&1 &
SERVER_PID=$!
echo "  PID: $SERVER_PID"

# Wait for ports
for i in $(seq 1 15); do
    http_ok=false; tcp_ok=false
    curl -s http://127.0.0.1:$HTTP_PORT/ > /dev/null 2>&1 && http_ok=true
    ss -tlnp "sport = :$TCP_PORT" 2>/dev/null | grep -q "$SERVER_PID" && tcp_ok=true
    if $http_ok && $tcp_ok; then
        echo "  服务器就绪 (${i}s, HTTP+TCP)"
        break
    fi
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
        echo "  ERROR: 服务器启动失败"
        echo "  --- 日志 ---"
        cat "$SERVER_LOG"
        exit 1
    fi
    sleep 1
done

echo ""
echo "[3] 检查启动后的数据目录"
echo "  文件列表:"
ls -la "$DATA_DIR/" 2>/dev/null | sed 's/^/    /'
echo ""

# Step 4: Install wallet
echo "[4] 安装 wallet 应用"
check "app-install" mcli --http-addr 127.0.0.1:$HTTP_PORT app-install --mpk "$MPK_FILE"
echo ""

# Step 5: Check server alive
echo "[5] 服务器存活检查"
if kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "  服务器进程存活"
else
    echo "  ERROR: 服务器已崩溃"
    echo "  --- 日志 ---"
    cat "$SERVER_LOG"
    exit 1
fi
echo ""

# Step 6: List apps
echo "[6] 列出已安装应用"
check_contains "list-apps" "wallet" curl -s http://127.0.0.1:$HTTP_PORT/mudu/app/list
echo ""

# Step 7: SQL query via mcli
echo "[7] SQL 查询测试 (通过 TCP)"
check_contains "SELECT users" "cannot find column\|user_id\|MError" mcli command --json "{\"app_name\":\"wallet\",\"sql\":\"SELECT * FROM users\"}" --compact --no-table
echo ""

# Step 8: Check server still alive
echo "[8] 服务器存活检查"
if kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "  服务器进程存活"
else
    echo "  ERROR: 服务器已崩溃"
    echo "  --- 日志 ---"
    cat "$SERVER_LOG"
    exit 1
fi
echo ""

# Step 9: HTTP invoke
echo "[9] 过程调用测试 (create_user)"
invoke_result=$(mcli --addr 127.0.0.1:$TCP_PORT --http-addr 127.0.0.1:$HTTP_PORT app-invoke \
    --app wallet --module wallet --proc create_user \
    --json '{"user_id":3,"name":"Charlie","email":"charlie@test.com"}' 2>&1)
echo "  Result: $invoke_result"

# Check server alive after invoke
if kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "  服务器存活"
else
    echo "  ERROR: 服务器在 invoke 后崩溃"
    echo "  --- 日志 ---"
    cat "$SERVER_LOG"
fi
echo ""

# Step 10: Final log
echo "[10] 服务器日志"
cat "$SERVER_LOG" | sed 's/^/    /'
