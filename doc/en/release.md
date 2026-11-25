# Release and Publishing

MuduDB releases are split into three stages, each with exactly one
implementation shared by local runs and CI:

```
package (local, safe)          verify + push (needs credentials)
-----------------------------  -------------------------------
cargo make package-release     GitHub Release  ← .github/workflows/build-release.yaml (tag v*)
cargo make build-sdk           GitHub SDK repos← .github/workflows/sdk-publish.yaml (manual)
cargo make package-sdk-registry                ├ npm    (@mududb/mududb, tag as-v*)
                               └ script/release/publish.py ┤ NuGet  (mududb, tag cs-v*)
                                                             └ PyPI   (mududb, tag py-v*)
```

Stage rules:

- **Packaging never pushes.** It is idempotent, needs no credentials, and its
  output is a directory with a manifest and per-file SHA-256 sums.
- **Publishing never compiles.** `script/release/publish.py` consumes the
  packaged directory, re-verifies hashes and tag↔version consistency, checks
  whether the version already exists (and skips it if identical), and only
  then uploads. It is a dry-run unless `--apply` is passed.
- **Triggers, tags, and secrets live in CI workflows**, not in the scripts.

## Server release (GitHub Release)

- Local: `cargo make package-release [--version vX.Y.Z] [--target <triple>]
  [--skip-build]` → `target/release-artifacts/<version>/` (tarball + sha256 +
  changelog). This is the same code the Release workflow runs
  (`script/release/package_release.py`), so local and CI packages cannot drift.
- CI: pushing a `v*` tag (or manually dispatching the Release workflow) builds
  the matrix, packages, and publishes the GitHub Release.
- Versioning: server tags are date-based (`vYYYYMMDD.HHMM` fallback for manual
  runs); the binaries themselves report their crate semver via `--version`.

## Registry packages (npm / NuGet / PyPI)

Each language binding publishes independently, with its own semver and tag
prefix (`as-v*`, `cs-v*`, `py-v*`; see `doc/en/binding_api_surface.md`).

```bash
cargo make package-sdk-registry            # pack all three into target/sdk-dist/registry/
python3 script/release/publish.py npm      # verify + rehearse (dry-run)
python3 script/release/publish.py npm --tag as-v0.2.0 --apply   # real upload
```

- Dry-run equivalents: `npm publish --dry-run`, a local-folder NuGet feed,
  `twine check`.
- Credentials for real uploads: `NODE_AUTH_TOKEN` (npm), `NUGET_API_KEY`
  (NuGet), `TWINE_USERNAME`/`TWINE_PASSWORD` or `PYPI_TOKEN` (PyPI). In CI the
  registries use trusted publishing (OIDC) where available; the PyPI upload in
  CI stays with `pypa/gh-action-pypi-publish`, the token path above is for
  manual publishes.
- Republishing an already-published identical version is a successful no-op;
  publishing different content under an existing version is refused.

## SDK distribution repositories and adapter runtime (GitHub)

`cargo make build-sdk` builds the per-language distribution repositories into
`target/sdk-dist/` (optionally `--with-adapter-runtime`). Publishing is manual
via the SDK Publish workflow, which calls `script/sdk/publish.py`
(distribution repos) and `script/sdk/publish_runtime.py` (adapter runtime
archives). Both are also reachable through the unified entry:
`script/release/publish.py github-sdk [--expected-commit <sha>]` and
`... github-runtime --artifacts <dir>`.

## Local rehearsal

Everything except the actual registry/GitHub upload can be exercised locally:

```bash
cargo make package-release --skip-build --version v0.0.0-rehearsal
cargo make package-sdk-registry
python3 script/release/publish.py npm,nuget,pypi   # full verify + dry-run
```

The bindings workflow also has a `workflow_dispatch` `dry_run` input that runs
the publish jobs with the same rehearsal behavior in CI.
