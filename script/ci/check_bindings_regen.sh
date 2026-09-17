#!/usr/bin/env bash
# Regenerate the cross-language binding codecs with mgen and verify the
# checked-in generated files are byte-identical (the "regen is clean" gate).
#
# The WIT files under crates/common/mudu_binding/wit/ are the single source of
# truth for the binding surface; whenever they (or the mgen templates) change,
# the generated files in all language bindings must be regenerated in the same
# commit. See doc/dev/binding_api_surface.md for the full recipe.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WIT_DIR="${REPO_ROOT}/crates/common/mudu_binding/wit"
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

cd "${REPO_ROOT}/crates/tools"
cargo build -q -p mudu_gen --bin mgen
MGEN="${REPO_ROOT}/target/debug/mgen"

fail=0
check() { # check <checked-in file> <generated file>
    if ! diff -q "$1" "$2" >/dev/null 2>&1; then
        echo "STALE: $1"
        fail=1
    fi
}

# C#: uni/*.cs (namespace mududb.types) + mudu_sys/UniSyscall.cs (mududb.codec)
"${MGEN}" message -i "${WIT_DIR}" -o "${TMP}/cs" -l csharp --with-func-codec
for f in "${TMP}"/cs/Uni*.cs; do
    b="$(basename "$f")"
    if [ "${b}" = "UniSyscall.cs" ]; then
        check "${REPO_ROOT}/crates/sdk/mudu_api/csharp/mudu_sys/${b}" "$f"
    else
        check "${REPO_ROOT}/crates/sdk/mudu_api/csharp/uni/${b}" "$f"
    fi
done

# Python: mududb/generated/*.py (+ the schema descriptor copied from the host codec)
"${MGEN}" message -i "${WIT_DIR}" -o "${TMP}/py" -l python --with-func-codec
for f in "${TMP}"/py/*.py; do
    b="$(basename "$f")"
    check "${REPO_ROOT}/crates/sdk/bindings/python/mududb/generated/${b}" "$f"
done
check \
    "${REPO_ROOT}/crates/sdk/bindings/python/mududb/generated/syscall_schema.desc.json" \
    "${REPO_ROOT}/crates/common/mudu_binding/src/codec/syscall_payload/syscall_schema.desc.json"

# AssemblyScript: assembly/generated/*.ts — the checked-in files live one
# directory below mpack.ts, so the regen recipe rewrites the mpack import
# path; everything else must be byte-identical.
"${MGEN}" message -i "${WIT_DIR}" -o "${TMP}/as" -l assemblyscript --with-func-codec
for f in "${TMP}"/as/*.ts; do
    b="$(basename "$f")"
    if ! sed 's|from "./mpack"|from "../mpack"|' "$f" |
        diff -q "${REPO_ROOT}/crates/sdk/bindings/assemblyscript/assembly/generated/${b}" - >/dev/null 2>&1; then
        echo "STALE: ${REPO_ROOT}/crates/sdk/bindings/assemblyscript/assembly/generated/${b}"
        fail=1
    fi
done

# Go: types/Uni*.go (package types; wire.go is hand-written and not regenerated)
"${MGEN}" message -i "${WIT_DIR}" -o "${TMP}/go" -l go --with-func-codec
for f in "${TMP}"/go/*.go; do
    b="$(basename "$f")"
    check "${REPO_ROOT}/crates/sdk/bindings/go/types/${b}" "$f"
done

# C: mududb/types/Uni*.h (mududb/codec is hand-written and not regenerated)
"${MGEN}" message -i "${WIT_DIR}" -o "${TMP}/c" -l c --with-func-codec
for f in "${TMP}"/c/*.h; do
    b="$(basename "$f")"
    check "${REPO_ROOT}/crates/sdk/bindings/c/mududb/types/${b}" "$f"
done

if [ "${fail}" -ne 0 ]; then
    echo
    echo "Generated binding files are stale. Regenerate with mgen and commit the"
    echo "results (recipe: doc/dev/binding_api_surface.md, section 'Regeneration')."
    exit 1
fi
echo "regen check: all generated binding files are up to date"
