#!/bin/bash
# Regenerate Rust entities for all examples and re-run the transpiler.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MGEN="$ROOT/target/debug/mgen"

(cd "$ROOT/crates/tools" && cargo build --bin mgen --quiet)

# wallet: entities only (procedures.rs / tests are hand-written)
"$MGEN" entity \
    --input-source-files "$ROOT/crates/sdk/example/wallet/sql/ddl.sql" \
    --output-source-folder "$ROOT/crates/sdk/example/wallet/src/rust" \
    --type-desc "$ROOT/crates/sdk/example/wallet/package/type.desc.json" \
    --lang rust

# app1: hand-maintained wallets entity mirrors the wallet schema
"$MGEN" entity \
    --input-source-files "$ROOT/crates/sdk/example/wallet/sql/ddl.sql" \
    --output-source-folder "$ROOT/target/tmp/app1_entity" \
    --type-desc "$ROOT/target/tmp/app1_entity/type.desc.json" \
    --lang rust
cp "$ROOT/target/tmp/app1_entity/wallets.rs" "$ROOT/crates/sdk/example/app1/src/rust/wallets.rs"

# tpcc
"$MGEN" entity \
    --input-source-files "$ROOT/crates/sdk/example/tpcc/sql/ddl.sql" \
    --output-source-folder "$ROOT/crates/sdk/example/tpcc/src/rust" \
    --type-desc "$ROOT/crates/sdk/example/tpcc/package/type.desc.json" \
    --lang rust

# vote
"$MGEN" entity \
    --input-source-files "$ROOT/crates/sdk/example/vote/sql/ddl.sql" \
    "$ROOT/crates/sdk/example/vote/sql/type.sql" \
    --output-source-folder "$ROOT/crates/sdk/example/vote/src/rust" \
    --type-desc "$ROOT/crates/sdk/example/vote/package/type.desc.json" \
    --lang rust

transpile() {
    local example="$1" wasm="$2"
    (cd "$ROOT/crates/sdk/example/$example" && python3 "$ROOT/script/build/transpiler.py" \
        --config build-cfg/transpiler-cfg.toml \
        --source src/rust \
        --target src/generated \
        --artifact src/artifact \
        --wasm-file "$ROOT/target/wasm32-wasip2/release/$wasm" \
        --package-desc package/package.desc.json \
        --type-desc package/type.desc.json >/dev/null)
}

transpile wallet wallet.wasm
transpile app1 mod_0.wasm
transpile tpcc tpcc.wasm
transpile vote vote.wasm

(cd "$ROOT/crates/sdk" && cargo fmt --all)
echo "regenerate-examples: OK"
