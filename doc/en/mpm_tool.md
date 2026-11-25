# mpm-tool: Package Inspect, Convert, and Sign

`mpm-tool` inspects, converts, and signs MuduDB application packages. It works
on the flat `.mpk` CAF artifact produced by `mpm-build create` (and the
`.mpk.zip` interchange form); see the
[CAF `.mpk` profile contract](contract/caf_mududb_profile_v1.md) for the
container format itself.

## Subcommands

| Command | Purpose |
|---|---|
| `verify <input>` | Verify a flat `.mpk` artifact: root/manifest hashes, component and payload counts, and — when signed — signature validity and signer fingerprints. |
| `inspect <input>` | Human-readable summary of a flat `.mpk` (runs `verify`). |
| `ls <input>` | List the payload files (`package.cfg.json`, `package.desc.json`, `ddl.sql`, `initdb.sql`, `abi.json`) and components of a flat `.mpk`. |
| `export <input> -o <out>` | Export a flat `.mpk` to `.mpk.zip`. |
| `import <input> -o <out>` | Import a `.mpk.zip` back to a flat `.mpk`. |
| `keygen` | Generate a fresh ed25519 signing seed and print its public key and fingerprint. |
| `new <name> [-d <dir>]` | Scaffold a new package project directory (`package.cfg.json`, `package.desc.json`, `ddl.sql`, `initdb.sql`). |

## Examples

```bash
mpm-tool verify wallet.mpk
mpm-tool inspect wallet.mpk
mpm-tool ls wallet.mpk
mpm-tool export wallet.mpk -o wallet.mpk.zip
mpm-tool import wallet.mpk.zip -o wallet.mpk
mpm-tool keygen
mpm-tool new my-app -d my-app
```

## Package signing

`mpm-build create` and `mpm-build create-from-toml` accept `--sign-seed <seed>`
(a 64-hex-char ed25519 seed; falls back to the `MUDUDB_MPK_SIGN_SEED`
environment variable) to sign the package. `mpm-tool keygen` prints a matching
`seed` / `public key` / `fingerprint` triple; the fingerprint is what the
server trusts.

To require signed packages, set the trusted fingerprints in `mudud.cfg`:

```toml
trusted_key_fingerprints = ["<fingerprint>"]
```

When non-empty, `mudud` rejects package install/load without a valid signature
from a trusted fingerprint.

## Related packaging tools

- **`mpm-build`** (`crates/tools/mpm_build`):
  - `create -a <package-cfg> -p <package-desc> -d <ddl-sql> -i <initdb-sql>
    -w <wasm>.. [-o <out>] [--sign-seed <seed>]` — build a flat `.mpk`
    (+ `.mpk.zip`) from the given package config, procedure description, DDL,
    initial data, and WASM module(s). `-w` is repeatable.
  - `create-from-toml -t <toml>` — build from a TOML argument file describing
    the package.
  - `merge-desc -f <input-folder> -d <output-desc-file>` — merge several
    `*.desc.json` procedure-description files into one.
- **`mpm-install`** (`crates/tools/mpm_install`): `mpm-install [--cfg FILE]
  [-s ADDR] [PACKAGE]`. `-s/--server` overrides the HTTP management address
  (default `127.0.0.1:8300`); `--cfg` points at an `mpm.cfg` TOML with
  `server` / `package` keys. Resolution order is `--cfg` → `./mpm.cfg` →
  `~/.mududb/mpm.cfg`, and command-line flags take precedence. The `.mpk`
  suffix is appended automatically when omitted.
