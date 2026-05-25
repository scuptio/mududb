#!/bin/bash
# Run wallet CRUD tests against a running MuduDB server.
# Prerequisites: mudud must be running, wallet.mpk must be in the mpk path.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
MPK="$ROOT/testing/mpk/wallet.mpk"
HTTP=8300

pass=0
fail=0

invoke() {
    curl -s -X POST "http://127.0.0.1:$HTTP/mudu/app/invoke/wallet/wallet/$1" \
        -H "Content-Type: application/json" -d "$2"
}

query() {
    mcli command --json "{\"app_name\":\"wallet\",\"sql\":\"$1\"}" --compact --no-table 2>&1
}

check_json() {
    local desc=$1 expected=$2
    shift 2
    echo -n "  [$desc] "
    output=$("$@" 2>&1)
    if echo "$output" | grep -q "$expected"; then
        echo "OK"
        ((pass++))
    else
        echo "FAIL (expected '$expected')"
        echo "    got: $output"
        ((fail++))
    fi
}

echo "=== Wallet CRUD Tests ==="

# Install app
echo ""
echo "[1] Install wallet"
mcli --http-addr 127.0.0.1:$HTTP app-install --mpk "$MPK" 2>&1 | head -1

echo ""
echo "[2] CRUD Operations"

check_json "CREATE user 3" '"ok":true' invoke create_user '{"user_id":3,"name":"Charlie","email":"charlie@test.com"}'

check_json "READ users" "Alice" query "SELECT user_id, name FROM users"
check_json "READ wallets" "10000" query "SELECT user_id, balance FROM wallets"

check_json "DEPOSIT +5000" '"ok":true' invoke deposit '{"user_id":1,"amount":5000}'
check_json "READ Alice balance" "15000" query "SELECT balance FROM wallets WHERE user_id=1"

check_json "TRANSFER 3000" '"ok":true' invoke transfer_funds '{"from_user_id":1,"to_user_id":2,"amount":3000}'
check_json "READ post-transfer" "12000" query "SELECT balance FROM wallets WHERE user_id=1"

check_json "DELETE user 3" '"ok":true' invoke delete_user '{"user_id":3}'
check_json "READ final users" "Alice" query "SELECT name FROM users"

echo ""
echo "=== Results: $pass passed, $fail failed ==="
