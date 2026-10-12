#!/usr/bin/env python3
"""Enforce the crate-layering rules described in ARCHITECTURE.md.

The repository is split into four group workspaces (crates/{common,db-kernel,
sdk,tools}) that form a strict dependency layering:

    common     (foundation: mudu, types, sys abstraction, parser, client)
    db-kernel  (kernel, runtime, raft, server, adapter, testing)
    sdk        (mududb SDK + examples)          ┐
    tools      (cli, gen, transpiler, mpm)      ┘ peers

This script resolves the internal dependency graph via `cargo metadata --no-deps`
for each group workspace and asserts that no crate depends "upward" (a lower
layer depending on a higher layer) or across the sdk/tools peer boundary. The
`mudu` crate is additionally required to depend on nothing at all. The resolved
internal graph is also checked for cycles.

Exit 0 = layering is clean; exit 1 = a forbidden edge or cycle was found.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
# (manifest dir relative to the repository root, architectural layer). The
# `example/mpk` consumer workspace is part of the `sdk` layer.
WORKSPACES = [
    ("crates/common", "common"),
    ("crates/db-kernel", "db-kernel"),
    ("crates/sdk", "sdk"),
    ("example/mpk", "sdk"),
    ("crates/tools", "tools"),
]

# (from_layer, to_layer) pairs that are forbidden. A lower layer may never
# depend on an upper layer, and the sdk/tools peers may not depend on each
# other.
FORBIDDEN_LAYER_EDGES = {
    ("common", "db-kernel"),
    ("common", "sdk"),
    ("common", "tools"),
    ("db-kernel", "sdk"),
    ("db-kernel", "tools"),
    ("sdk", "tools"),
    ("tools", "sdk"),
}

# `mudu` is the pure foundation crate; it must not depend on any other
# repository crate.
NO_DEPENDENCIES = {"mudu"}

# Known, documented exceptions. Keep this minimal: every entry must carry a
# reason and should be removed once the underlying edge is refactored away.
ALLOWED_EDGES = {
    # `testing` (db-kernel) shells out to the compiled `mudu_cli` binary to
    # drive interactive sessions in integration tests. This is a dev-only,
    # process-level dependency, not a library dependency.
    ("testing", "mudu_cli"),
}

# Fine-grained layering of the extracted kernel crates. Lower rank = lower
# layer. A kernel crate may depend only on kernel crates of equal or lower
# rank; `catalog` (rank 0) is the shared leaf and must not depend on any
# other kernel crate. This mirrors the bottom-up extraction order in
# ARCHITECTURE.md.
KERNEL_RANKS = {
    "catalog": 0,
    # Test-fixture crate sitting directly on `catalog`; consumed only through
    # dev-dependencies (in-memory meta-mgr fixture).
    "catalog-testing": 1,
    "index": 1,
    "wal": 1,
    "xengine": 2,
    "storage": 3,
    "meta": 4,
    "command": 5,
    "sql": 6,
    "conn": 7,
    "xcontract": 8,
    "worker": 9,
    "repl": 10,
    "backup": 11,
    "server": 12,
}


def resolve_graph():
    """Return (name_to_layer, edges, normal_edges).

    `edges` covers every declared dependency kind (normal, dev, build), while
    `normal_edges` covers normal (non-dev, non-build) dependencies only. Cycle
    detection runs on `normal_edges`, because dev/build back-edges (e.g. a
    crate's test harness depending on the kernel facade) do not form a build
    cycle.
    """
    name_to_layer = {}
    edges = set()
    normal_edges = set()
    for subpath, layer in WORKSPACES:
        manifest = ROOT / subpath / "Cargo.toml"
        proc = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps",
             "--manifest-path", str(manifest)],
            capture_output=True,
            text=True,
            check=True,
        )
        data = json.loads(proc.stdout)
        for package in data["packages"]:
            name_to_layer[package["name"]] = layer

    for subpath, _layer in WORKSPACES:
        manifest = ROOT / subpath / "Cargo.toml"
        proc = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps",
             "--manifest-path", str(manifest)],
            capture_output=True,
            text=True,
            check=True,
        )
        data = json.loads(proc.stdout)
        for package in data["packages"]:
            name = package["name"]
            for dep in package.get("dependencies", []):
                dep_name = dep.get("name")
                if dep_name in name_to_layer:
                    edges.add((name, dep_name))
                    if dep.get("kind") in (None, "normal"):
                        normal_edges.add((name, dep_name))
    return name_to_layer, edges, normal_edges


def find_cycles(edges):
    """Return a list of dependency cycles in the internal graph.

    Each cycle is reported as a list of crate names in dependency order
    (the last name depends back on the first). One representative cycle is
    kept per distinct set of involved crates.
    """
    adj = {}
    for src, dst in edges:
        adj.setdefault(src, []).append(dst)

    cycles = []

    def walk(path, node, on_stack):
        for nxt in adj.get(node, ()):
            if nxt in on_stack:
                idx = path.index(nxt)
                cycles.append(path[idx:] + [nxt])
            else:
                on_stack.add(nxt)
                path.append(nxt)
                walk(path, nxt, on_stack)
                path.pop()
                on_stack.remove(nxt)

    for start in sorted(adj):
        walk([start], start, {start})

    unique = []
    seen = set()
    for cycle in cycles:
        key = tuple(sorted(set(cycle)))
        if key not in seen:
            seen.add(key)
            unique.append(cycle)
    return unique


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()

    name_to_layer, edges, normal_edges = resolve_graph()
    violations = []

    for (src, dst) in sorted(edges):
        if src in NO_DEPENDENCIES:
            violations.append(f"{src} (foundation) must not depend on {dst}")
            continue
        src_layer, dst_layer = name_to_layer[src], name_to_layer[dst]
        if (src_layer, dst_layer) in FORBIDDEN_LAYER_EDGES and (src, dst) not in ALLOWED_EDGES:
            violations.append(f"{src} ({src_layer}) must not depend on {dst} ({dst_layer})")
        if src in KERNEL_RANKS and dst in KERNEL_RANKS:
            if KERNEL_RANKS[dst] > KERNEL_RANKS[src]:
                violations.append(
                    f"{src} (kernel rank {KERNEL_RANKS[src]}) must not depend on "
                    f"{dst} (kernel rank {KERNEL_RANKS[dst]})"
                )

    for cycle in find_cycles(normal_edges):
        violations.append("dependency cycle: " + " -> ".join(cycle))

    if violations:
        print("crate layering violations:")
        for violation in violations:
            print(f"  - {violation}")
        return 1

    print("crate layering is clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
