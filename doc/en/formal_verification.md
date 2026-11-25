# Aeneas + Lean Formal Verification Plan

*中文版本： [formal_verification.zh-CN.md](../zh-CN/formal_verification.zh-CN.md)*

This document defines the project structure, pilot scope, and maintenance process for
introducing Aeneas + Lean into MuduDB.
Phase 1 has delivered the extraction of the production length-prefixed buffer, the Lean
specification and proofs, and the Python install/verification scripts.
See the [verification README](../../verification/README.md) for the operational entry points.
Tool checks and root-free installation are integrated into
`cargo make doctor` / `cargo make doctor -- install --only formal-verification`;
the verification entry points are `cargo make verify`, `verify-extract`, and `verify-self-test`.
Official tool bundles with pinned, SHA256-verified hashes are used, avoiding any OPAM/OCaml
system dependency.
The registry now also covers the production RecordSlot codec/fragment core, checked page
layout arithmetic, and WAL recovery/GC decisions. Their physical proof directories mirror
`record_slot_core.rs`, `layout_math.rs`, and `layout/decision.rs`. These additions retain the
public APIs and disk format. See the verification README for precise assumptions and the
Rust regression boundaries; filesystem recovery itself is not claimed to be formally verified.
Future module expansion continues to follow this plan.

## Goals and Boundaries

The approach is "production Rust as the single source of truth, an independent Lean proof
project, and CI checking that code and proofs stay in sync", verifying safe, sequentially
executed, I/O-free algorithmic cores first.

```text
production Rust → Charon → LLBC → Aeneas → Lean generated implementation
                                           + behavioral specification and proofs
                                           → Lean kernel checking
```

Aeneas only performs the translation; functional correctness still requires maintainers to
define specifications and complete the proofs. Lean checks formal propositions, so the linkage
from production code to the generated model, the extraction configuration, and the assumptions
about external dependencies must all be stated as well.

According to the current official documentation, unsafe and concurrency remain limitations of
Aeneas's supported scope. Phase 1 does not cover the whole database, and local algorithm proofs
do not replace integration tests, fuzzing, or the internal deterministic simulation. In
particular: a recovery algorithm being correct under a given persistence model does not mean
power-loss recovery on a real file system has been proven.

## Directory Structure and Naming

The existing Cargo workspace grouping is kept. The production-code mirror directories under
`verification/lean/` must match the original repository paths, preserving the `crates`, group,
crate, `src`, and module hierarchy; when `example/` is covered in the future it is mirrored by
its original path as well. New directories and files use the project's existing `snake_case`
style; existing names (for example `db-kernel`) are kept as-is.

Do not use `Generated/`, `Spec/`, or `Proof/` at the Lean root to split a single production
module apart. The generated implementation, specification, and proofs live inside the
corresponding module directory.

```text
verification/
  README.md
  toolchain.lock.toml
  targets.toml
  lean/
    lean-toolchain
    lakefile.toml
    lake-manifest.json
    crates/
      common/
        mudu/
          src/
            common/
              endian/
                generated.lean
                spec.lean
                proof.lean
            utils/
              buf/
                generated.lean
                spec.lean
                proof.lean
      db-kernel/
        storage/
          src/
            page/
              record_slot/
                generated.lean
                spec.lean
                proof.lean
        wal/
          src/
            worker_wal_backend/
              layout/
                gc/
                  generated.lean
                  spec.lean
                  proof.lean
    support/

script/verification/
  toolchain.py
  check.py
  test_verification.py

doc/en/formal_verification.md
```

Path mapping rules:

```text
crates/<group>/<crate>/src/<path>/<module>.rs
    ↓
verification/lean/crates/<group>/<crate>/src/<path>/<module>/
```

For a Rust directory module `<module>/mod.rs`, use the corresponding `<module>/` directory,
with submodules mirrored further down. Proofs specific to the crate root `lib.rs` go into the
corresponding `src/lib/`, to avoid mixing them with other modules. When a Rust module is moved
or renamed, update the proof directories and the target manifest in sync.

| File or directory | Responsibility |
| --- | --- |
| `generated.lean` | Automatic translation of the production implementation; manual edits forbidden |
| `spec.lean` | Behavioral definitions, preconditions, and postconditions independent of the implementation |
| `proof.lean` | Proofs that the generated implementation satisfies the specification |
| `external.lean` | External dependency models added as needed; the associated assumptions must be recorded |
| `support/` | Lemmas and proof helpers shared across modules; must not hold a second copy of business logic |
| `targets.toml` | Mapping of Rust entry points, extraction configuration, Lean theorems, assumptions, and coverage status |
| `toolchain.lock.toml` | Custom version manifest read by the verification scripts; not a tool-native lock file |

`generated.lean` is the logical entry point the project expects; do not assume Aeneas can emit
output directly at this granularity. The first extraction should confirm how types, functions,
and external templates are actually split, and the script then organizes the files and their
import relationships deterministically. If shared declarations can only be generated per crate,
place them in a `generated/` directory under that crate's mirror directory and reference them
from the module entry points; definitions must not be copied, nor the generated output
hand-modified.

Physical directories keep original names such as `db-kernel`. If Lean imports, module
identifiers, or namespaces need escaping, handle it uniformly in the generation script and
validate it through the Lake build; mirror directory names must not be quietly changed just to
accommodate identifiers. Standard file names required by the tooling, such as
`lean-toolchain`, are kept as-is.

## Production Code Organization Principles

1. Production Rust is the only source of implementation. Prefer extracting the target
   functions and their dependencies directly; do not maintain verification-only copies of
   algorithms.
2. Run small-scale extraction experiments first, then adjust dependencies based on the actual
   obstacles encountered. Do not perform large-scale refactoring for anticipated tool
   limitations.
3. Isolate pure logic by module first; split a crate only when genuinely necessary, naming it
   after the domain, for example `page_format`.
   Production code must call the verified implementation — verifying an unintegrated substitute
   version is not acceptable.
4. Database-specific algorithms stay in the `db-kernel` layer; they are not moved into the
   pure foundation library `mudu` just because formal verification needs it.
5. If a separate extraction entry point is needed, it must reference the same production code,
   and its features, cfg flags, target platform, and dependencies must be recorded. Any
   divergence between the verification configuration and the production configuration requires
   review; cfg must not be used to swap out the actual algorithm.
6. New crates follow `ARCHITECTURE.md`: update the architecture checks and keep
   cross-workspace dependencies rlib-only.
   When modifying a workspace manifest, account for the internal DS overlay baseline update,
   and do not introduce dependencies on the internal simulation backend.

## Phase 1: Length-Prefixed Buffer

The first targets are `write_sized_buf` and `read_sized_buf` in
`crates/common/mudu/src/utils/buf.rs`, which together with the necessary endianness helpers
form a minimal closed loop. They are short, I/O-free, and have a clear input/output
relationship, making them well suited for gauging the toolchain and the cost of proof
maintenance.

The initial source review identified the following edge cases. Phase 1 handles them through
explicit error types, length checks, and the corresponding Lean proofs:

- The representable range of `src.len() as u32`, and the overflow behavior of the length plus
  the 4-byte header.
- What is returned when the destination buffer is too small, and whether the output is left
  unchanged.
- The error semantics of an insufficient header versus an insufficient payload.
- The old `read_sized_buf` documentation described the error value as the expected total
  length, while the implementation returned the payload length `n`;
  the new interface uniformly uses `BufferTooSmall.required` for the total length — see the
  verification README for details.

First batch of properties:

| Property | Target contract |
| --- | --- |
| Encode/decode roundtrip | With sufficient capacity and a representable length, reading the written result back yields the original payload |
| Consumed length | On success, the consumed length is exactly `4 + payload.len()` |
| Write range | Success modifies only the target region; failure leaves the output untouched |
| Truncation handling | An insufficient header and an insufficient payload each return their agreed error |
| Arithmetic safety | Under the stated preconditions there is no overflow, out-of-bounds access, or panic |
| Success and termination | On valid input the function actually succeeds and terminates, rather than only proving the result of the success branch |

The specification must distinguish mathematical natural numbers from Rust fixed-width
integers, and state the target bit width of `usize`.
Do not exclude invalid inputs that the API is supposed to handle through overly strong
preconditions; for such inputs, prove that the agreed error is returned.
If reasonable behavior cannot be expressed under the existing return type, review an API
adjustment first.

The acceptance criteria for Phase 1 are: make the contracts above explicit, complete the
production code extraction, prove the agreed properties, wire in a reproducible CI check, and
register the remaining assumptions and the uncovered scope.

## Subsequent Expansion Order

| Phase | Target | Main properties |
| --- | --- | --- |
| 1 | buffer and endianness conversion | roundtrip, length, bounds, error semantics |
| 2 | `storage/src/page/record_slot.rs` | encode/decode roundtrip, reserved bits, non-target fields unchanged |
| 3 | page space allocation and record interval computation | no overlap, no out-of-bounds, capacity conservation |
| 4 | WAL pure recovery decisions and GC boundary computation | state invariants, deletion conditions |
| Later | index operations, transaction state machine | compositional correctness and more complex invariants |

`RecordSlot` already has roundtrip and fragment-flag tests that can serve as a specification
reference for Phase 2, but dependencies such as `byteorder` and the error macros still need
their extraction support checked first. Do not declare these dependencies as unconditionally
correct external functions just to eliminate proof obligations.

The WAL phase should separate pure decision-making from I/O execution, and explicitly list the
assumptions about persistence, call ordering, and execution success.
Actual I/O, concurrency, and crash behavior remain covered by integration tests, fault
injection, and the internal DS verification.

## Toolchain and Generated Implementation Maintenance

- Pin the Aeneas commit, its matching Charon commit, the Rust toolchain needed for extraction,
  the Lean version, and the Lake dependencies.
  The Lean version should match the Aeneas backend; do not track the latest version of each
  tool independently.
- Do not change the Rust toolchain used for normal development. Extraction builds explicitly
  use a separate directory, for example `target/verification/charon/`, and keep the
  repository's existing jobs and test-parallelism limits.
- Commit the generated Lean files so semantic changes can be reviewed; LLBC, Lake build
  caches, and the like are treated as build artifacts.
- Automatically generated files carry a Lean-appropriate comment header, for example
  `-- Generated by script/verification/check.py using Aeneas. Do not edit manually.`.
- The generation script only cleans up the generated implementation it owns; it must not
  overwrite `spec.lean`, `proof.lean`, or hand-written external models.
- Toolchain upgrades are committed separately: re-extract and run all registered proofs,
  avoiding mixing them with business-logic changes.

The official basic command chain is as follows; the project scripts additionally set the
working directory, tool versions, extraction scope, and output paths explicitly:

```text
charon cargo --preset=aeneas
aeneas -backend lean <input.llbc>
lake build
```

Day-to-day operations should use the cargo make entry points from the README, with the
configuration pinned by the Python scripts. When external models are needed, the official
`-split-files` mechanism can generate templates for the missing definitions, and the
corresponding models and assumptions are then maintained.

## CI and Day-to-Day Development Flow

`script/verification/check.py` wires together the full chain:

```text
verify the toolchain and the extraction configuration
  → regenerate Lean from the current production Rust
  → check that the generated implementation matches the committed version (including added and removed files)
  → build all registered specification and proof modules
  → check the axiom dependencies of the key theorems and the coverage list
```

Running only `lake build` may keep verifying a stale generated implementation; checking only
the generated implementation does not prove the properties either.
The build entry point must import all registered proof modules, so that newly added files
cannot escape the build. The verification runner enforces this: a preflight stage fails when
the `lakefile.toml` roots do not cover every module registered in `targets.toml`, instead of
surfacing later as a missing-object-file error.

After the pilot is complete, set up a blocking gate for modifications to code already in the
verified scope. Incremental triggers must include the transitive dependencies of the target
functions, the Cargo lock files, features, the extraction scripts, and the toolchain manifest;
run full verification periodically as a supplementary check.
Rust fmt, clippy, and tests continue to follow the current repository process.

Two advisory/day-to-day aids join the gate: `cargo make verify-premises` re-runs the tests
cited by premise `established_by` references so a renamed, moved, ignored, or failing test
rots the trust chain loudly, and `cargo make verify-spec-diff -- --diff <ref>` prints a
declaration-level diff of changed spec/proof Lean files (added/removed/changed signatures)
so reviewers can focus on whether a postcondition weakened. The latter is advisory by
design: weakening is a human judgment, the aid only focuses it. New targets start from
`cargo make verify-new-target`, which scaffolds the registry entry, extraction module, and
spec/proof skeletons with an embedded authoring guide.

When modifying covered code, update the implementation, specification, generated
implementation, and proofs in the same PR; review whether specification changes weaken the
original guarantees. If a refactoring preserves the contract, keep the specification as stable
as possible and update only the implementation linkage and the proofs.

## Proof Quality and Trust Boundaries

- Specifications describe observable behavior rather than restating the implementation line by
  line. Review whether preconditions are reasonable and satisfiable.
- For key theorems, check the dependency closure reported by `#print axioms`, rejecting
  `sorryAx` and unreviewed project axioms.
  Standard logical axioms and external-model assumptions must be explicitly registered;
  merely searching for `sorry` is not enough to audit a proof.
- AI may assist in writing proofs, but acceptance is still based on toolchain checks,
  specification review, and assumption audits.
- External models must record the corresponding Rust definitions, the applicable versions, the
  semantic justification, and the unproven linkage.
- Lean kernel checking does not automatically prove the Rust compiler, Charon, the Aeneas
  implementation, or external models correct;
  the verification report must state these trust boundaries.
- Measure coverage by "entry functions, extraction configuration, proven properties,
  assumptions, uncovered scope"; do not substitute proof line counts
  or the number of generated Lean functions for quality metrics.

`targets.toml` registers the source paths and symbols, the transitive-dependency
extraction scope, features/cfg, the target platform, the generated modules, the theorem
names, the sanity theorems, the assumptions, the owner, the premises with their
discharging Rust tests, and the status. The statuses distinguish "extracted" and "proven" —
a successful translation is never treated as completed verification, and a proven target
must carry at least one audited sanity theorem showing the specification succeeds on
concrete inputs. The runner validates the whole registry (schema, premise references,
unique mutation anchors), rejects proof-weakening constructs (`sorry`, `admit`, project
`axiom` declarations, `native_decide`, `implemented_by`) in every committed Lean file, and
supports incremental target selection by name or by changed files (`script/verification/impact.py`),
with the pre-commit hook running the extraction-drift gate on staged changes.

## References

- [Project architecture](../../ARCHITECTURE.md)
- [Aeneas official repository: installation, supported scope, and external models](https://github.com/AeneasVerif/aeneas)
- [Aeneas Lean tutorial](https://github.com/AeneasVerif/aeneas/tree/main/tests/lean/Tutorial)
- [Aeneas proof project overview](https://github.com/AeneasVerif/aeneas/blob/main/documentation/aeneas-overview.md)
- [Lean compilation pipeline and kernel checking](https://lean-lang.org/doc/reference/latest/Elaboration-and-Compilation/)

Tool support scope changes over time; when this is actually adopted, the documentation at the
pinned commits and the actual extraction results are authoritative.

The WAL P0 extension extracts append reservations and frame/prefix bounds from
production code. It proves append cursor preservation, pairwise reservation
separation, overflow-safe frame intervals, irreversible prefix rejection, and
accepted-byte conservation by composition. Validation outcomes and a valid initial
append cursor are explicit premises; CRC, migration and filesystem effects remain
outside the proof boundary. See `verification/README.md` for the integration scope.

The P1 targets extend this to multipart sequencing/byte accumulation, truncation
and reset decisions, ordered sequence edits/page publication/payload relocation,
and per-key page deltas with replay gating. These are extracted production cores
with explicit composition proofs. Sorted merge traversal, concrete page wiring,
filesystem effects and concurrency remain regression-tested integration boundaries;
see the P1 section of `verification/README.md` for premises and runtime costs.
