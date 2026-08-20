#!/bin/sh
# Linker wrapper: link with the rust-lld bundled in the active toolchain
# instead of the system GNU ld. rust-lld uses far less memory (and is faster)
# when linking large test binaries such as the wasmtime-dependent ones in
# this workspace, which keeps `cargo test` within tight memory budgets.
#
# The toolchain ships `ld.lld` under lib/rustlib/<host>/bin/gcc-ld/ exactly so
# that a cc driver can find it via `-B <dir> -fuse-ld=lld`. If that directory
# is missing (unexpected toolchain layout), fall back to the default linker.
set -eu

sysroot="$(rustc --print sysroot)"
host="$(rustc -vV | sed -n 's/^host: //p')"
lld_dir="$sysroot/lib/rustlib/$host/bin/gcc-ld"

if [ -d "$lld_dir" ]; then
    exec cc -B"$lld_dir" -fuse-ld=lld "$@"
else
    exec cc "$@"
fi
