# SDK distributions

中文发布操作指南：[使用 GitHub Actions 发布多语言 SDK](sdk_publish_github_actions.zh-CN.md)。

The main repository is the source of truth. `cargo make build-sdk` builds isolated
source distributions for `mududb/mududb-as`, `mududb/mududb-py`,
`mududb/mududb-cs`, `mududb/mududb-go`, `mududb/mududb-c`, and
`mududb/mududb-rs`. There is no cargo-make publishing task.

## Language and execution modes

All six languages have SDK distribution builds. Guest procedures run in `mudud`; native adapter examples run as local applications and load a separate adapter runtime. These modes have different entry points and do not imply identical APIs.

| Language | Guest MPK example | Native adapter entry |
|---|---|---|
| AssemblyScript | [wallet-as](../../example/mpk/wallet-as/readme.md) | [Node adapter](../../crates/sdk/bindings/assemblyscript/adapter/) |
| Python | [wallet-py](../../example/mpk/wallet-py/readme.md), componentize-py | [Python adapter](../../crates/sdk/bindings/python/mududb/adapter.py) |
| C# | [wallet-cs](../../example/mpk/wallet-cs/readme.md) | [NativeAdapter](../../crates/sdk/mudu_api/csharp/mudu_sys/NativeAdapter.cs) |
| Go | [wallet-go](../../example/mpk/wallet-go/readme.md), TinyGo | [Go adapter](../../crates/sdk/bindings/go/adapter/) |
| C | [wallet-c](../../example/mpk/wallet-c/readme.md), wasi-sdk | [C adapter](../../crates/sdk/bindings/c/mududb/adapter/) |
| Rust | [wallet](../../example/mpk/wallet/readme.md) | [Standalone SDK adapter](../../crates/sdk/mudu_api/rust/src/adapter.rs) |

The Rust guest example uses the repository's `mududb` facade; the published Rust SDK is the separate `mudu_api_rust` package. C and Go guest examples supply their host-call bridges; their type/codec bindings alone are not a full guest database facade. See [Wallet: MPK and native adapter modes](#wallet-mpk-and-native-adapter-modes) for the exported examples and their build requirements.

## Local builds

```bash
cargo make build-sdk
SDK_LANGS=py,go cargo make build-sdk
# Optional output override, including when the shared target is not writable:
SDK_LANGS=c SDK_OUTPUT_DIR=/tmp/sdk-dist cargo make build-sdk
```

## Prerequisites

**The default command builds all six SDKs, so all six languages' build tools must
be installed.** Building includes compilation, tests, packaging, and installation
into an isolated consumer project; it does not just copy source files. The command
checks for missing tools but does not install them automatically.

To use only the toolchains already installed, select languages explicitly:

```bash
# Skip Go and Python SDKs:
SDK_LANGS=as,cs,c,rs cargo make build-sdk
# Build just the C SDK (common prerequisites below are still required):
SDK_LANGS=c cargo make build-sdk
```

### Common prerequisites (every language selection)

The build targets native Linux, macOS, and Windows. Common prerequisites are
Git, Python 3.11+, a native C/C++ toolchain, the Rust toolchain pinned in `rust-toolchain.toml`, and cargo-make. Rust is required even
when `rs` is not selected: every build compiles mgen and runs the generated-code
consistency check. Build orchestration uses Python and native executables, with
no Bash, Make, sed, or diff requirement. Toolchain installers and other project
tasks may have their own requirements.

`cargo make build-sdk` uses `python3` on Unix and `python` on Windows. Override
`SDK_PYTHON` with a Python executable path (not a command plus arguments). All
Python subprocesses use that same interpreter. Direct invocation also works:

```bash
python3 script/sdk/sdk_build.py --languages py,go --output target/sdk-dist
```

Windows PowerShell (the output and interpreter paths may contain spaces):

```powershell
python -m venv .venv/sdk-build
.venv/sdk-build/Scripts/python.exe -m pip install build
$env:SDK_PYTHON = "$PWD/.venv/sdk-build/Scripts/python.exe"
$env:SDK_LANGS = "py,go"
$env:SDK_OUTPUT_DIR = "$PWD/target/sdk-dist"
cargo make build-sdk
```

No virtual-environment activation is necessary when `SDK_PYTHON` is set. Remove
`SDK_LANGS` to return to all six languages.

For Ubuntu 24.04, install the common native build dependencies:

```bash
sudo apt-get update
sudo apt-get install -y git curl ca-certificates build-essential pkg-config \
  python3 python3-pip python3-venv liburing-dev clang libclang-dev llvm-dev
```

Install Rust through [rustup](https://rustup.rs/) if it is not already available.
Then run these commands from the repository root; `rustup show` installs the
repository-pinned toolchain and components if needed:

```bash
rustup show
cargo install cargo-make --locked
python3 --version
cargo --version
cargo make --version
```

On macOS, install Xcode Command Line Tools (`xcode-select --install`), Git,
Python 3.11+, and rustup. Install the selected languages using their native
installers; Node 22, .NET 8 SDK, and Go provide macOS packages.

On Windows, install Git for Windows, Python 3.11+ (on PATH), rustup with the MSVC
host toolchain, LLVM (including `clang.exe` and `clang++.exe` on PATH), and Visual
Studio Build Tools with the **Desktop development with C++** workload and Windows
SDK. Run from a Developer PowerShell so the compiler can find MSVC headers and
libraries. Git Bash and WSL are unnecessary. Install Node 22, the .NET 8 SDK and
Go as needed, then reopen the terminal and verify their paths with `Get-Command`.
The standard .NET installer uses `$env:ProgramFiles/dotnet`; the selected host
must expose the .NET 8 runtime in `dotnet --list-runtimes`.

Stale generated files must be regenerated and committed by the developer; the
SDK build never repairs them silently. `CARGO_TARGET_DIR` is honored by the
regeneration check.

### Language-specific prerequisites

Install only the rows corresponding to `SDK_LANGS`, or all rows for the default
build. Version checks must succeed in the **same shell** that runs cargo-make.

| Selection | Required tools | Check |
|---|---|---|
| `as` | Node.js 22 and npm; AssemblyScript is installed by `npm ci` | `node --version` and `npm --version` |
| `py` | Python 3.11+ with venv and the PyPA `build` package | `python3 -m build --version` |
| `cs` | .NET 8 SDK, including its runtime (a runtime-only install is insufficient) | `dotnet --list-sdks` and `dotnet --list-runtimes` |
| `go` | Go 1.24 or newer | `go version` |
| `c` | clang and clang++; Windows additionally needs MSVC headers/libraries | `clang --version`, `clang++ --version` |
| `rs` | Pinned Rust toolchain and `wasm32-wasip2` target | `rustup target list --installed` |

**AssemblyScript:** use the [Node.js installation guide](https://nodejs.org/en/download)
to install Node 22 and npm. If you already use nvm:

```bash
nvm install 22
nvm use 22
node --version
npm --version
```

**Python:** install packaging tools in a virtual environment, avoiding changes to
the operating system's Python packages. Run from the repository root and keep
this environment activated when invoking `cargo make build-sdk`:

```bash
python3 -m venv .venv/sdk-build
source .venv/sdk-build/bin/activate
python3 -m pip install --upgrade pip build
python3 -m build --version
```

Install `build` into the interpreter selected by `SDK_PYTHON` or the platform
default; installing it into a different environment will not resolve the error. Installing
`build` only through pipx also does not make it importable by this interpreter.

**C#:** follow the [.NET Ubuntu installation guide](https://learn.microsoft.com/en-us/dotnet/core/install/linux-ubuntu-install)
for your distribution's package feed. Alternatively, install the SDK in your home
directory with Microsoft's installer:

```bash
curl -fsSL https://dot.net/v1/dotnet-install.sh -o /tmp/mududb-dotnet-install.sh
bash /tmp/mududb-dotnet-install.sh --channel 8.0 --install-dir "$HOME/.dotnet"
export DOTNET_ROOT="$HOME/.dotnet"
export PATH="$DOTNET_ROOT:$PATH"
hash -r
dotnet --list-sdks
dotnet --list-runtimes
```

The C# tests target `net8.0`: the selected `dotnet` host must list
`Microsoft.NETCore.App 8.0.x`. A host with only .NET 10 can compile the tests
but cannot run them with the default runtime selection. The build checks this
before exporting SDKs and reports the host path and visible runtimes. If .NET 8
is already installed under `$HOME/.dotnet` but `command -v dotnet` selects
`/snap/bin/dotnet`, use the `DOTNET_ROOT` and `PATH` exports above; reinstalling
is unnecessary. Verify with `dotnet --list-runtimes` in the same shell used to
run `cargo make build-sdk`.

**Go:** download and install a Go 1.24+ toolchain for your OS/architecture using
[Go's official installation instructions](https://go.dev/doc/install). Distribution
packages may provide an older release; check `go version` before building. If Go
is already installed but the command cannot be found, add its actual `bin`
directory to `PATH`. For the common system or user-local installation locations:

```bash
export PATH="$HOME/.local/go/bin:/usr/local/go/bin:$PATH"
command -v go
go version
```

Use the path where you installed Go if it differs, and add the export to your
shell profile for future terminals. The native Go toolchain is needed here;
TinyGo is not required for this SDK distribution build.

**Rust:** from the repository root, install/check the guest target:

```bash
rustup target add wasm32-wasip2
rustup target list --installed
```

These prerequisites are for SDK distributions. Building wallet guest components
via `cargo make package-examples` is a separate task with additional toolchains
such as TinyGo, componentize-py, or wasi-sdk.

## Build after installation

```bash
# Run in the shell where the Python virtual environment and toolchain PATHs
# above are active:
cargo make build-sdk
```

If `Missing SDK build tools: go, Python build` appears, check `go version` and
`python3 -m build --version` in this shell, or exclude `go` and `py` with
`SDK_LANGS`. The manual SDK Publish Action installs its prerequisites on the
runner; local builds require you to install them yourself.

Dependencies may be downloaded; an offline build is not promised. Builds run
sequentially, with at most four Rust build jobs and test threads. Consumer
validation happens outside the source checkout.

## Build output

Each `target/sdk-dist/mududb-<language>/` contains a clean `repo/` tree and
`artifacts/` (source archive, applicable language packages, and SHA256SUMS).
`manifest.json` identifies exactly the SDKs built in that invocation, with source
commit, dirty flag, stable package versions, tool versions, validation results
and file hashes. Only a fully successful batch gets a manifest. Unselected old
output directories are not part of that manifest and are not published.

AS, Python, C#, and Rust versions come from their package manifests; C and Go
versions are in `script/sdk/config.json`. Currently only stable X.Y.Z versions
are accepted. Change versions in source before building; immutable published tags
cannot be reused for different source commits or contents.

SDKs contain the committed golden corpus and support running tests outside the
monorepo. They expose the capabilities documented by each binding, not identical
capabilities in every language. Guest syscalls require a compatible MuduDB host.
The Rust distribution uses the standalone `mudu_api_rust` package, not the
internal `mududb` facade. Consumers can name that dependency `mududb` in Cargo.
The Go distribution's canonical module is `github.com/mududb/mududb-go`.

## Manual GitHub Action

Open **Actions → SDK Publish → Run workflow**, select the source ref and languages.
`publish` defaults to false: this builds, verifies and uploads an Actions Artifact
for Linux, macOS, and Windows (`sdk-distributions-Linux`,
`sdk-distributions-macOS`, `sdk-distributions-Windows`). There are no push, PR or
scheduled triggers. All three build jobs must succeed before publication; only
the Linux artifact is published. Platform archives are not required to have
identical bytes. To publish, explicitly enable
`publish`; optionally enable `create_repos` to create missing public repositories.

Configure the `SDK_PUBLISH_TOKEN` Actions secret with access to the six target
repositories and Contents write permission. Automatic organization repository
creation additionally requires Administration write permission and organization
policy approval. Ordinary publishing does not need repository creation rights.
The default workflow token is used only for reading the source/artifacts.

The publishing job writes the secret into a mode-0600 temporary file, passes
its path through `SDK_GITHUB_TOKEN_FILE`, then removes it. Local token files may
be placed at `.secrets/github-sdk.token` (ignored by Git); never commit tokens.
The token is not stored in remote URLs, command arguments, artifacts or logs.
No credentials are available to the build job.

Publication uses the build artifact without rebuilding it, requires a clean
source commit matching the workflow SHA, verifies its file hashes, and prechecks
all existing repositories before writing. It appends managed commits on `main`,
creates `vX.Y.Z` tags, and attaches the packages to a GitHub Release. The Release
stays draft until uploads finish. Publishing runs are serialized and are not
cancelled by newer runs.

Use empty repositories on first publication. Existing content without the managed
marker, manual changes to managed trees, concurrent branch updates, changed
existing versions and mismatched assets fail instead of being overwritten.
No force pushes, tag replacement, registry publishing or repo deletion occurs.
A 404 can also mean insufficient token access; check permissions before enabling
repository creation.

GitHub cannot atomically publish six repositories. The publish-report Artifact
records attempted outcomes. Retry the failed job using the same build artifact:
matching tags are reused, already uploaded assets are digest-checked, missing
assets are uploaded and the draft Release is completed. Asset collisions without
a matching GitHub SHA-256 digest require inspection; they are never overwritten.

## Maintenance and tests

Export allowlists are in `script/sdk/config.json`; build and publishing helpers
are under `script/sdk/`. The regeneration gate is
`python script/ci/check_bindings_regen.py`; its old `.sh` path is a compatibility
wrapper. The C distribution ships `check.py`, runnable with `python check.py`
without Make. AS validation calls the local compiler through Node directly;
standard Node/npm and nvm layouts are supported. If `npm-cli.js` cannot be found,
install a standard Node distribution with npm instead of using an opaque shim. Generated release README files direct contributions back
to this main repository. Generated Go imports are changed through mgen, together
with scaffold templates and local examples.

```bash
python3 -m unittest discover -s script/sdk/tests -v
```

These publication tests use disposable local Git repositories and fake GitHub API
responses. They do not create repositories, push to GitHub or require a token.

### Build diagnostics

The AS package exports classes for AssemblyScript consumers, not as Wasm exports;
its compiler configuration disables only AS235 for those intentional exports.
AS210 remains enabled: generated option/result encoders use the local value
after an explicit null guard, without redundant non-null assertions. Other
compiler diagnostics remain enabled. C# retains XML documentation generation
and malformed-comment diagnostics, while CS1591 completeness checking is opt-in
with `-p:EnforceCompleteXmlDocumentation=true` until generated and hand-written
APIs have full documentation. Exported C# snapshots embed source files in symbols
and disable Git/SourceLink discovery, because their paths do not match the original
repository. Normal monorepo builds retain SourceLink.

## Wallet: MPK and native adapter modes

Every exported SDK includes `examples/wallet/README.md` and
`README.zh-CN.md`, shared business sources under `mpk/`, and a local application
entry under `adapter/`. `mpm build` in the Wallet directory produces
`target/wallet-<language>.mpk`. Install the matching `mpm`, `mtp`, `mgen`, and
`mpm-build` binaries first. `mpm build --manifest-path PATH --python PYTHON`
selects a project and interpreter; otherwise `MPM_PYTHON`, then platform Python,
is used. `mpm.toml` contains `[build]`, `script`, and an optional `args` array;
paths resolve relative to the manifest, not the caller's current directory.

The Wallet illustrates calls, not a production accounting system. Multi-statement
transfers inherit backend transaction behavior; balance prechecks do not provide
concurrent atomicity. Do not infer transaction guarantees from the example.

```sh
cargo make build-sdk
cargo make build-sdk --with-adapter-runtime
SDK_LANGS=py,go cargo make build-sdk --with-examples
cargo make build-sdk --with-examples --example-mode adapter
```

The default always exports both examples and docs but does not require their
extra toolchains. `--with-examples` follows `SDK_LANGS`/`--languages`, implies a
native runtime build, and validates both modes.

### MPK mode

`--example-mode mpk` validates only packages against a real disposable Mudud
server. The complete MPK matrix is Linux x64; the C# guest currently requires
that host. Guest toolchain versions are listed in the
[toolchain matrix](#toolchain-matrix) below.

### Native adapter mode

`--example-mode adapter` avoids Wasm tools. The native runtime builds in an
isolated target directory, configurable with `SDK_ADAPTER_TARGET_DIR`. It
requires Rust, a native C/C++ toolchain, CMake, Node.js and Clang.
`NODE_INCLUDE_DIR` can select installed Node-API headers; otherwise the builder
downloads headers for the current Node version. Windows also needs the MSVC
build environment and the matching Node import library (downloaded by the
builder). Ordinary SDK source exports contain no runtime binary and do not
download Node headers.

Archives and checksums are under `adapter-runtime/`, named with the Rust host
triple. Extract the archive and set `MUDU_ADAPTER_LIBRARY` to its library's
absolute path. Keep `mudu_adapter.node` beside it for AS. The adapter executes
business logic locally without invoking the installed procedures.

### Connection strings

Configure `MUDU_CONNECTION` before process startup:

- `sqlite://wallet.db`
- `postgres://user:password@localhost/wallet`
- `mysql://user:password@localhost/wallet`
- `mudud://127.0.0.1:9527/wallet-py?http_addr=127.0.0.1:8300`

The connection chooses a backend, not an MPK execution mode. Mudud requires an
existing application/schema. For the Wallet, build and install its MPK once to
bootstrap that schema, then skip the adapter's `init` command and run `scenario`.
Test each mode with a separate fresh database or Mudud data directory; the
examples never silently delete pre-existing data.

### Toolchain matrix

| Example component | Required tools |
|---|---|
| Native adapter runtime | Rust, a native C/C++ toolchain, CMake, Node.js and Clang |
| MPK guest — all languages | `wasm-tools` 1.252.0 |
| MPK guest — Python | `componentize-py` 0.25.1 |
| MPK guest — Go | TinyGo 0.42.0 and a compatible Go toolchain |
| MPK guest — C | wasi-sdk 29 (`WASI_SDK`) |
| MPK guest — C# | .NET 10 |
| Adapter — C# | .NET 8 |
| Adapter — AS | `npm ci` in the SDK root |
| Adapter — Go | cgo and a C compiler |

### Per-language notes

C# shares Wallet business functions between modes, but its WASI guest retains
the existing lightweight AOT codec bridge: MessagePack-CSharp requires runtime
facilities unavailable in that guest. The native example uses the distributed
SDK facade and real adapter. Rust uses a local MP2 adapter around the standalone
SDK, without depending on the internal workspace facade. C's native demonstration
is a short-lived command-line process; its copied guest row arena is
process-scoped and is not intended as a long-lived server allocator.

### SDK Publish Action options

The manual SDK Publish Action adds `with_adapter_runtime` and `with_examples`,
both disabled by default. All platform jobs build native runtime attachments
when enabled; adapter examples run on all platforms and MPK examples on Linux.
Runtime publication additionally requires `publish`. It creates a separate main
repository Release named `sdk-runtime-v<version>-<source-commit-prefix>` and
checks archive digests and source provenance before uploading. SDK repository
publication remains controlled by the existing separate publisher. No push or PR
trigger is added.
