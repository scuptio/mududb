#!/usr/bin/env bash
# Build the py-spike guest package (spike.mpk) from source.
# Requires: componentize-py (pipx install componentize-py==0.25.1), wasm-tools, zip.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${HERE}/../../../.." && pwd)"
cd "${HERE}"

componentize-py -d wit -w spike componentize app \
    -p . -p "${REPO_ROOT}/crates/sdk/bindings/python" \
    -o spike.wasm
wasm-tools validate spike.wasm

rm -f spike.mpk
zip -q spike.mpk package.cfg.json package.desc.json ddl.sql initdb.sql spike.wasm
echo "built ${HERE}/spike.mpk ($(stat -c%s spike.mpk) bytes)"
