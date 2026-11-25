# Production Rust Verification: Installation, Checks, and Troubleshooting

*中文版本： [README.zh-CN.md](README.zh-CN.md)*

The verification registry covers the length-prefixed buffer, RecordSlot's disk codec and
fragment updates, page-space arithmetic, pure WAL recovery/GC decisions, the MSSP wire
codec (MessagePack primitives and the 16-byte frame header), and the pure Raft decision
cores (quorum arithmetic and vote predicates). Production
Rust is imported directly into the extraction crate; there are no hand-written replacement
implementations or project-specific axioms.

## Storage and WAL guarantees

| Target | Checked properties |
| --- | --- |
| `record_slot_core` | 32-byte little-endian layout, field roundtrip, reserved data, fragment mask, short-buffer rejection, unchanged output suffix |
| `layout_math` | Checked slot-array bounds, payload intervals, eight-byte alignment, padding-inclusive capacity conservation, separation from earlier records |
| `decision` | Checked identifier successor, recovery at or above the persisted frontier, tail reuse, archive-limited GC bounds, strict chunk deletion predicate |

The P0 extensions also cover the production `append_plan::reserve`,
`frame_bounds::bounds`, and `Prefix::advance` functions:

- Append reservations preserve the cursor invariant, keep normal payloads inside
  their chunk, isolate oversized payloads, and consume fresh sequence numbers
  without wrapping. Zero-length reservations preserve the cursor. Sequence
  exhaustion returns an error before the caller changes any append state.
- Frame bounds account for the 24-byte header and 8-byte trailer without overflow.
  Both initial parsing and post-migration splitting check these bounds before slicing.
- The prefix cursor advances by exactly the accepted frame length and cannot resume
  after rejection. A composition theorem over the extracted step proves that retained
  bytes equal the sum of the maximal accepted prefix, including empty/truncated input.
- A composition theorem for two successful positive reservations proves increasing
  chunk sequences or disjoint intervals in the same chunk.

The append proofs assume a valid starting cursor (current sequence below the next
sequence, size within capacity, and zero size when there is no current chunk).
The prefix composition takes validation outcomes as inputs: it does not prove CRC,
version migration, or the entire Rust parser loop. Rust tests additionally truncate a
real multi-frame stream at every byte boundary and check the retained frames.
The production integrations, format constants, and regression tests connect these
pure guarantees to the I/O wrappers; no filesystem durability claim is implied.

The public RecordSlot API and disk layout are unchanged. Page writers use the same allocation
math as the extracted code. Rebuilds now preflight all records before changing the page;
capacity failure therefore preserves the original bytes. Invalid slot counts are rejected
before slicing. WAL recovery reports identifier exhaustion instead of wrapping or reusing a
value. An empty valid prefix still resumes at the persisted GC frontier. GC stops at the first
retained or unclassifiable chunk, preserving a contiguous deletion prefix; an archive frontier
behind the old GC frontier authorizes no further deletion.

The WAL proofs take validated frame summaries and persisted frontiers as inputs. They do not
prove the CRC implementation, directory enumeration, filesystem persistence order, concurrent
GC, or end-to-end crash recovery. Page separation assumes previous intervals end at or before
the allocation cursor; it does not certify arbitrary existing pages as non-overlapping.
These integration boundaries remain covered by Rust regression tests.

## Wire codec guarantees

| Target | Checked properties |
| --- | --- |
| `mp_core` | Canonical MessagePack primitives: per-constructor decode-of-encode roundtrips consuming exactly the encoded bytes out of any suffixed input, minimal-width marker selection on encode, short-input and foreign/reserved-marker rejection, the signed/unsigned chain split |
| `mssp_frame` | 16-byte MSSP frame header layout: header field roundtrip over appended bodies (magic, version, zero flags, message kind), short-input rejection |

The byte-level MessagePack codec is `mudu_binding::universal::mp_core`, shared verbatim by
the host and the standalone guest SDK (the `mudu_api_sync` mirror). The `Value` dispatch,
UTF-8 validation, and float bit conversion live in the `mp_wire` caller outside the proved
boundary; they are pinned as premises by the byte-for-byte `rmp_serde` comparison tests and
the golden corpus (`testing/fixtures/golden/v1/`). A full recursive `Value` roundtrip is
deliberately not claimed: the per-constructor roundtrips compose through the decoder's
consumed-byte counts, and `Int` values with non-negative payloads canonically decode back as
`UInt`.

New handwritten Lean specifications and proofs include English comments explaining the
contract, premises, and key steps. `db-kernel` directories retain their production names;
Lean imports spell that path segment as `«db-kernel»`.

## Raft consensus guarantees

| Target | Checked properties |
| --- | --- |
| `raft_quorum` | Position-aligned grant counting with leader exclusion (equals the specification count, never exceeds the voter count), strict-majority check, high-median majority commit-index pick over a complete descending list, quorum intersection (two strict majorities over the same voter list share a non-excluded granting voter) |
| `raft_vote` | Log freshness as a lexicographic (term, index) comparison, full characterization of the vote-grant predicate, single-vote-per-term safety (a recorded vote rejects any other candidate in the same term), granted votes never go to an older term |

The Raft targets cover the pure decision cores extracted into `mudu_raft::core::quorum_core`
and `mudu_raft::core::vote_core`. The `HashMap` grant collection, the descending sort, and
the `RaftCore` state plumbing (`current_term`, `voted_for` persistence, log head) stay in
`quorum.rs`/`election.rs` and are pinned as premises by the quorum and election tests.
Protocol-level liveness and full state-machine safety are out of scope for extraction;
they remain the domain of the simulation tests (and, prospectively, TLA+-style model
checking).

## P1: composition of WAL and page operations

| Production target | Verified contract |
| --- | --- |
| `part_chain` | Continuations decrement the remaining-part counter exactly; only zero completes an entry; accumulation preserves existing and incoming bytes; two deliveries compose without loss or duplication |
| `truncate_plan` | The boundary frame is discarded, only strictly later chunks are removed, and the reset cursor retains the cut offset with a fresh representable sequence; composition locates the first cut at a whole-frame boundary |
| `sequence_edit` | Splicing preserves the prefix/replacement/suffix exactly; ordering follows from ordered parts and boundary relations; publication is all-or-nothing; each payload relocation preserves all bytes outside its planned interval |
| `delta_value` | Per-key diff followed by apply reproduces the new value; other keys are unchanged; pointwise logical-page roundtrip and LSN-gated replay idempotence |

The Rust page mutation methods now prepare a 4 KiB draft on the stack and publish it
only after every fallible edit succeeds. The input buffer's suffix is preserved.
This adds a page copy per mutation; no throughput improvement is claimed. The existing
allocation proofs supply the range and separation contracts for rebuilds. Generic
sequence proofs explicitly require value-preserving `Clone`, as used by byte and slot
callers. Ordered-edit proofs require the retained parts and inserted records to satisfy
the comparator's boundary relations.

The delta page model assumes unique keys and a sound payload equality comparison.
The production sorted merge traversal, sorting library, header/slot wiring, and filesystem
operations are integration boundaries, not a proof of every line in those wrappers.
The first-cut composition takes validated frame lengths and LSNs; monotone LSN order is
needed to interpret the retained prefix as all records below a boundary. Truncation
requires the existing exclusive-use precondition (no commit/promote in flight).
Replay idempotence assumes a successfully constructed image is stamped with its batch
LSN; the proofs do not establish crash-atomic filesystem writes or fsync ordering.

Regression tests exercise all multipart delivery split points, rejected continuation
followed by a valid retry, live truncate/append/reopen, mixed page edits against a reference
record map, unchanged pages on failure, and generated delta reconstruction including
insertion, update, deletion, and page links. All targets have independent mutation tests.

## Environment and Prerequisite Dependencies

The first release supports **Linux x86_64, including x86_64 WSL2**. Run WSL2
builds inside the Linux file system. macOS, native Windows, and ARM Linux do not have
automated install support yet.

You need Python 3.12+ (with `tomllib` and safe tar extraction support), Git, Rustup, a C
compiler, GNU tar, zstd, curl, and CA certificates. Reserve at least 20 GiB of disk; the first
download includes the Lean compiler, Rust compiler components, the Mathlib source, and proof
caches, so the time depends on your network. Builds use at most
four Rust jobs and four Lean threads by default.

On Ubuntu / Debian, the system dependencies are installed by the user (doctor does not run
these commands):

```bash
sudo apt update
sudo apt install python3 git build-essential tar zstd curl ca-certificates
```

If you do not have Rustup yet, install it into your user directory following the
[official Rustup instructions](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup-init.sh
sh /tmp/rustup-init.sh -y --profile minimal
. "$HOME/.cargo/env"
cargo install cargo-make --locked
```

Skip the corresponding steps if you already have Rustup / cargo-make. There is no need to
manually install OPAM, OCaml, Nix, or Elan:
this project uses the official Aeneas release bundles (with the matching Charon and Lean
backend) and the official Lean compiler packages, verified against SHA256 and invoked through
their absolute paths. Compared with the original source-build plan, this reduces system-level
dependencies.

Versions are pinned in [toolchain.lock.toml](toolchain.lock.toml): Aeneas
`nightly-2026.10.24-74aabd5`, the matching Charon, Rust `nightly-2026-09-17`, and Lean
`4.35.0`.
Dependencies such as Mathlib are pinned in the [Lake manifest](lean/lake-manifest.json).
Floating latest versions are not tracked.

## First-Time Installation and Verification

Run from the repository root:

```bash
cargo make doctor
cargo make doctor -- install --only formal-verification --dry-run
cargo make doctor -- install --only formal-verification
cargo make verify
```

`cargo make doctor` only reports; it does not install. `install` downloads and verifies the
tool bundles, installs the pinned Rust components and the Lean dependency caches;
it never uses sudo and does not modify the default Rust toolchain or shell configuration.
It stops with a message when system dependencies are missing.
The install command can be re-run; already-installed items that match are skipped.
All steps are orchestrated by Python scripts, with no extra shell install scripts.

If cargo-make is not installed yet, you can also run the equivalent Python commands directly:

```bash
python3 script/doctor.py install --only formal-verification
python3 script/verification/check.py check
```

Output on success:

```text
PASS: production source extraction, Lean proofs and axiom audit
```

`verify` re-extracts from the current source into a temporary directory, compares the result
against the committed generated files, builds the Lean proofs,
and finally checks the axiom dependencies of all registered theorems.
Missing tools are never silently skipped, and nothing is installed automatically during
verification.
Do not conclude that production code is verified merely because a direct `lake build`
succeeds: it may be using a stale generated implementation.

## Code, Specification, and Directories

```text
crates/common/mudu/src/utils/buf.rs                 # production buffer implementation
verification/extract/                             # standalone Cargo extraction entry point
verification/lean/crates/common/mudu/src/utils/buf/
  generated.lean                                  # auto-generated; manual edits forbidden
  spec.lean                                       # mathematical specification
  proof.lean                                      # proofs
script/verification/                              # Python install/verify/test scripts
```

The extraction entry point references the same production module via `#[path]`, with no
features or verification-specific cfg.
Charon starts from the functions registered for each target. Generated implementations
depend on the Aeneas standard library models; no project external models were added.
Physical directories match the production code, including the `common` and `db-kernel` groups.

[targets.toml](targets.toml) registers the entry points, the platform, all audited theorems,
and the permitted standard logical axioms.

| Theorem | Guarantee |
| --- | --- |
| `length_correct` | The total length is the payload length plus four, or the overflow is reported exactly |
| `write_success` | The output is the big-endian header, the original payload, and the remaining tail of the original output buffer |
| `write_overflow`, `write_short` | The failure type is exact and the output is completely unchanged |
| `read_short_header` | Reports the available length when fewer than four bytes are present |
| `read_overflow` | Rejects a header that declares an unrepresentable total length |
| `read_truncated` | Reports the required total length when the payload is truncated |
| `read_success` | Returns the exact total length and the corresponding payload from the input |
| `decode_header_append` | The big-endian header encoding and decoding in the mathematical specification are mutually inverse |
| `roundtrip` | Writing then reading restores the original payload, and the read/write total lengths agree |

The Aeneas success postcondition additionally requires the function to return a successful
model result, rather than only describing "if it succeeds".
In the Rust API, `Err` is a normal return value, unlike a panic, out-of-bounds access,
overflow, or divergence in the model.

## API Migration and Compatibility

The write function used to return `0` for errors; it now returns
`Result<u32, SizedBufError>`.
The read error changed from `Option<u32>` to `SizedBufError`:

- `LengthOverflow`: the payload length plus four is not representable as `u32`.
- `HeaderTooShort { available }`: fewer than four bytes are available.
- `BufferTooSmall { required, available }`: insufficient capacity; `required` is the total
  length including the header.

Success lengths remain `u32`, and the maximum payload is `u32::MAX - 4`. Header presence is
checked first (on read), then total-length representability, and finally capacity.
Any write error leaves the output unmodified.
`sized_buf_len` lets callers check the length before allocating.
The string-encoding callers have been migrated; the big-endian binary format for valid inputs
is unchanged. This is a breaking change to the Rust source API — callers outside this
repository must handle the new return types.

## Day-to-Day Changes and Verification

```bash
# After modifying production code or specifications:
cargo make verify-extract
# Review the generated diff and update specs/proofs; do not edit generated.lean.
git diff -- verification/lean
cargo make verify

# Python gate tests:
python3 -m unittest discover -s script/verification -p 'test_*.py'

# Rust checks continue to use the repository's existing flow:
cargo make check
```

`verify-extract` only updates the generated implementation; it does not guarantee that the
proofs hold. Specifications should express the intended behavior — do not weaken premises or
results just to make a wrong implementation pass. Toolchain upgrades are committed separately:
update the checksums and the matched versions, then re-extract, re-prove, and re-run the
self-test.

## Registry Fields, Sanity Checks, and Incremental Runs

Every target in [targets.toml](targets.toml) now carries governance metadata that the
runner validates before doing any work:

| Field | Rule |
| --- | --- |
| `status` | `proven` (theorems and sanity checks audited) or `extracted` (drift-checked only); a successful translation is not a proof |
| `owner` | Non-empty owning area |
| `assumptions` | Trust-boundary notes that are believed but not machine-checked; the list must be present even when empty |
| `sanity` | Theorems proving the specification actually succeeds on concrete inputs; at least one is required for `status = "proven"`, and they are axiom-audited like the main theorems |
| `premises` | Proof preconditions the integration layer must establish, each with `established_by` references of the form `path` or `path::symbol` pointing at the Rust test or assertion that discharges it; unresolvable references fail the run |

The same preflight also fails fast when a registered `mutation_from` no longer matches its
source exactly once, and rejects proof-weakening constructs (`sorry`, `admit`, `axiom`
declarations, `native_decide`, `implemented_by`) in every committed `.lean` file — comments
are stripped first, so documenting the ban is still allowed.

Sanity theorems live at the end of each `proof.lean` as `sanity_*` theorems that instantiate
the target's proved contracts on concrete witnesses (e.g. `sanity_roundtrip` writes and reads
back one concrete payload). They close the "specification was weakened until vacuous" hole:
a spec that no concrete input can satisfy no longer compiles.

Target selection flags work on every action that needs the toolchain:

```bash
python3 script/verification/check.py check --targets buf,decision
python3 script/verification/check.py check --diff main          # targets affected by the diff
python3 script/verification/check.py drift --staged             # the pre-commit form
```

A change inside a target's crate affects that crate's targets; a change under
`crates/common/` or to the verification machinery itself affects everything (the mapping
lives in `script/verification/impact.py`, which also checks that the CI workflow's path
triggers cover every registered target).

More entry points:

```bash
cargo make verify-drift            # extract + compare only; no proof build
cargo make verify-determinism      # extract twice, require byte-identical Lean
cargo make verify-report           # trust-boundary report into target/verification/report/
python3 script/verification/check.py mutation-sweep --list-only   # list generated candidates
cargo make verify-mutation-sweep   # run generated mutants; report the kill rate (slow)
cargo make verify-premises         # run the tests cited as premise evidence
cargo make verify-spec-diff -- --diff main   # advisory declaration diff of spec/proof files
cargo make verify-new-target -- --name ... --source ... --owner ... --functions ... [--dry-run]
```

`verify-report` needs no formal toolchain and validates the registry, then writes
`verification-report.md` / `.json` with per-target status, owners, assumptions, premises
and their discharging tests, plus the standard trust-boundary statement.

The mutation sweep generates one single-site operator flip (`<`/`<=`, `>`/`>=`, `==`/`!=`,
`&&`/`||`) per eligible occurrence, skipping sites already covered by registered mutations
and Rust syntax such as `->`/`>>`. Each candidate is classified as killed by the proof,
killed at compile time, silent (extraction unchanged), or surviving; survivors indicate
specification blind spots. A full sweep is slow (one extraction plus proof build per
candidate), so bound exploration with `--budget` and run it manually or nightly — CI only
lists candidates as a smoke check.

`verify-premises` runs the tests cited by `established_by` premise references (mapping
`crates/<group>/<crate>/src|tests/...` paths to cargo invocations) and fails when a
referenced test was renamed, moved, ignored, or fails — the registry's existence check
alone cannot see that rot.

`verify-spec-diff` is the advisory spec-weakening review aid: it diffs the declaration
signatures of changed `spec.lean`/`proof.lean` files against a base ref and prints a
`::warning` in CI. Whether a changed postcondition weakens the contract stays a human
review decision; the aid focuses it.

## Authoring a new target

`cargo make verify-new-target` scaffolds the registration in one step: the extraction
`#[path]` module line, the `targets.toml` entry (starting at `status = "extracted"`), the
`spec.lean`/`proof.lean` skeletons, and the lakefile roots, then extracts and compiles the
skeleton. The generated `proof.lean` carries an authoring guide distilled from the codec
proofs. Its key points, in short:

- prefer arithmetic (`* 256 +`) over bitwise packing at proof boundaries; `>>` and `%` by
  constants stay val-level and are fine
- if-chains over `match` on scalar literals
- unrolled fixed-width reads over `from_be_bytes` arrays
- encoders as `&mut Vec<u8>` push/extend_from_slice; decoders as
  `&[u8] -> Result<(T, usize), Error>` returning consumed byte counts
- proof skeletons: `step`/`step with` through the WP monad, `scalar_tac` for capacity side
  conditions, `getElem!_pos` for slice indexing, `simp only […MARKER_X]` for marker
  constants, `WP.spec_mono` for sanity theorems

Before scaffolding, the production source must be dependency-free (the extraction
workspace has no dependencies) and written in the shapes above; refactor first if needed
(the codec verification did exactly this for `mp_core`/`mssp_frame`).

## Deliberately Introducing an Error: Does the Verification Really Work?

```bash
cargo make verify-self-test
```

The self-test first requires the normal source to pass full verification, then changes the
written length header to zero in a temporary copy under `target/verification/`.
Each registered mutation runs in its own isolated copy. The runner checks that re-extraction
produces a diff, then builds that target's original proof against the mutant implementation. The mutant implementation and the specification must compile
successfully first; only an error from the proof files counts as a successful detection.
The original source and the committed generated files are not modified.

Expected final output:

```text
PASS: every independent source mutation rejected by its production proof
```

This validates both layers: the source linkage and the proof gate. A formal proof failure
usually shows unsolved goals or type errors; it does not guarantee an automatically generated
executable counterexample — combine it with Rust unit tests to locate concrete inputs.

## Troubleshooting and Logs

Every `verify`, `verify-extract`, and `verify-self-test` run creates its own directory:
`target/verification/runs/<UTC time>-<action>-<random id>/`. The path is printed at the start
and at the end, so logs from different runs and from normal/mutant implementations never
overwrite each other. Install logs remain in `target/verification/logs/`.

The output keeps the full commands, working directories, and log paths, and additionally
provides:

- Stage numbering, RUNNING / PASS / FAIL status, and per-stage elapsed time; the stage count
  scales with the number of registered targets and self-test mutations.
- A waiting hint every ten seconds showing the stage being executed and the time used.
- The final goal, the number of successful stages, the number of verified functions, the
  number of audited theorems, and the total elapsed time; extraction-only commands state
  explicitly that proofs were not checked.
- The first compile error with its context, file/line/column information, the original last 25
  log lines, and handling suggestions.
- The self-test separately reports that the mutant implementation compiled and that the proof
  was rejected as expected; infrastructure errors are still treated as failures.

`output.log` holds the stage and command summary; `summary.json` holds the status, versions,
per-stage elapsed times, and errors.
`extract-charon.log`, `extract-aeneas.log`, `lean-build.log`, and `axiom-audit.log` hold the
complete tool output.
Re-running the same command uses a numeric suffix. The self-test additionally saves
`mutant-compile.log`, `mutant-proof.log`, and a `mutant-sources/` source snapshot so
diagnostics remain available after the temporary directory is cleaned up.
When the generated files are out of sync, the fresh generated implementation is kept as
`fresh-generated.lean`.

Terminal output reuses the project's color style; redirected output, CI, and `NO_COLOR` or
`TERM=dumb` environments use plain text.
Stage progress is updated as stages actually complete and does not estimate remaining time.

| Failure location | Meaning and handling |
| --- | --- |
| doctor reports missing system dependencies | Install them following the prerequisite steps above, then re-run doctor |
| Download failed | Check the network to GitHub, the Rust distribution site, and the Mathlib cache; re-run doctor install |
| SHA256 mismatch | Installation is refused; check your proxy or a corrupted download — do not directly modify the expected checksums |
| Missing tools or version drift | Run `cargo make doctor -- install --only formal-verification` |
| `extract-charon.log` | Rust compilation, extraction entry point, language support, or pinned toolchain problem |
| `extract-aeneas.log` | Aeneas translation failed; do not work around it with opaque declarations or axioms |
| `Generated Lean is stale` | The current Rust differs from the generated files; re-extract and review |
| `lean-build.log` | A theorem or specification does not match the generated implementation; check the source diff and unsolved goals |
| `axiom-audit.log` | Only `propext`, `Classical.choice`, and `Quot.sound` are allowed; any other dependency fails |
| Self-test mutant passes | The gate is broken or the specification was weakened; this must not be ignored as a normal success |

Tools, downloaded bundles, and temporary extractions live in `target/verification/`; Lean
packages and compile caches live in `verification/lean/.lake/`. Deleting these caches requires
re-installing/rebuilding; do not delete the committed Lean source files, the toolchain version
manifest, or the Lake manifest. The pinned toolchain managed by the system Rustup stays in the
user directory.

## Trust Boundaries and CI

The proofs target the pinned Linux x86_64 extraction configuration; they do not support
claiming that the whole database is verified. The Lean kernel, the Rust compiler, the
Charon/Aeneas translation implementation, and the Aeneas standard library models belong to the
trust boundary; the proofs do not cover memory allocation failure, the operating system,
concurrency, persistence, or disk failures. Rust tests, fuzzing, and integration tests are
still required.

A dedicated CI workflow reuses doctor and the same Python verification scripts, running
verification, the self-test, extraction determinism, and the script tests, and uploading
failure logs. Pull requests verify only the targets affected by the diff (computed by
`impact.py`); pushes, the nightly schedule, and manual runs always verify the full
registry. A separate step checks that the workflow's path triggers cover every registered
target source. The pre-commit hook additionally runs the drift gate on staged changes when
the formal toolchain is installed, and skips silently otherwise.
Plain `cargo make build` and the pre-commit hook do not implicitly install the formal tooling.
