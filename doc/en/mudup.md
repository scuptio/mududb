# mudup Design

`mudup` is the MuduDB toolchain installer and version manager. Its role is similar to
`rustup`: it downloads verified prebuilt releases, installs them into a per-user
toolchain directory, and exposes stable command names such as `mudud`, `mcli`, `mpk`,
`mgen`, and `mtp`.

## Goals

- Install MuduDB tools without requiring a local Rust build environment.
- Support timestamped release versions such as `v20260514.1144`.
- Support host-specific artifacts such as `x86_64-unknown-linux-gnu`.
- Allow users to install, update, switch, list, and remove toolchains.
- Keep installed versions isolated so rollback is simple.
- Verify downloaded artifacts before activating them.
- Start with a small command surface, then add components and overrides later.

## Non-goals

- `mudup` does not replace OS package managers for system libraries such as glibc.
- `mudup` does not build MuduDB from source in the first version.
- `mudup` does not manage application `.mpk` packages installed into a running
  `mudud` server. That remains an `mcli` responsibility.

## User Model

The first version should support these commands:

```bash
mudup install <vYYYYMMDD.HHMM>
mudup update
mudup list
mudup uninstall <vYYYYMMDD.HHMM>
```

`mudup update` installs the latest available version. Release versions use UTC timestamps
in the form `vYYYYMMDD.HHMM`; the latest version is the greatest timestamp known to the
release channel.

## Installation Layout

`mudup` uses a per-user root directory:

```text
${HOME}/.mudup/
  settings.toml
  downloads/
  tmp/
  bin/
    mudud
    mcli
    mpk
    mgen
    mtp
  toolchains/
    current -> <vYYYYMMDD.HHMM>
    <vYYYYMMDD.HHMM>/
      bin/
        mudud
        mcli
        mpk
        mgen
        mtp
      lib/
        lib-list.txt
      manifest.txt
```

The `current` entry is a symbolic link to the active versioned toolchain. Command proxies
or symlinks are installed into `${HOME}/.mudup/bin`, and that directory should be added to
`PATH`.

## Release Artifact Layout

Each release tarball should unpack to a single top-level version directory:

```text
<vYYYYMMDD.HHMM>/
  bin/
    mudud
    mcli
    mpk
    mgen
    mtp
  lib/
    lib-list.txt
  manifest.txt
```

`lib/lib-list.txt` is copied from `build-release/lib-list.txt` during release packaging.
It records dynamic library requirements and their download locations. `manifest.txt`
records the packaged files and release metadata.

## Distribution Server

The release server can be static HTTP storage. It does not need a dynamic API.

Suggested layout:

```text
https://scuptio.com/dist/releases/<vYYYYMMDD.HHMM>/mududb-<vYYYYMMDD.HHMM>-x86_64-unknown-linux-gnu.tar.gz
https://scuptio.com/dist/releases/<vYYYYMMDD.HHMM>/mududb-<vYYYYMMDD.HHMM>-x86_64-unknown-linux-gnu.tar.gz.sha256
```

Example channel manifest:

```toml
manifest_version = 1
channel = "stable"
latest = "vYYYYMMDD.HHMM"

[[releases]]
version = "vYYYYMMDD.HHMM"
date = "YYYY-MM-DD"

[[releases.artifacts]]
host = "x86_64-unknown-linux-gnu"
url = "https://scuptio.com/dist/releases/vYYYYMMDD.HHMM/mududb-vYYYYMMDD.HHMM-x86_64-unknown-linux-gnu.tar.gz"
sha256 = "<hex-sha256>"
```

Each artifact should include an installation manifest:

```text
name=mududb
version=vYYYYMMDD.HHMM
target=x86_64-unknown-linux-gnu
archive=mududb-vYYYYMMDD.HHMM-x86_64-unknown-linux-gnu.tar.gz
archive_uri=https://scuptio.com/dist/releases/vYYYYMMDD.HHMM/mududb-vYYYYMMDD.HHMM-x86_64-unknown-linux-gnu.tar.gz
bin_dir=bin
lib_dir=lib
lib_list=lib/lib-list.txt

[files]
bin/mudud
bin/mcli
bin/mpk
bin/mgen
bin/mtp
lib/lib-list.txt
manifest.txt
```

## Install Flow

`mudup install stable` should:

1. Detect the host triple.
2. Download the channel manifest.
3. Select the package for the host and profile.
4. Check prerequisites and report missing system dependencies.
5. Download the artifact to `${HOME}/.mudup/downloads`.
6. Verify SHA-256 and, when available, a release signature.
7. Extract the artifact into `${HOME}/.mudup/tmp`.
8. Validate the artifact manifest and required binaries.
9. Move the extracted toolchain into `${HOME}/.mudup/toolchains`.
10. Refresh command proxies and environment setup hints.

Activation must be atomic. A failed download, checksum mismatch, or extraction failure
must not modify the current default toolchain.

## Update Flow

`mudup update` should:

1. Fetch the current remote channel manifest.
2. Compare the installed version with the remote latest version.
3. Download and verify only changed artifacts.
4. Install the new toolchain beside the old one.
5. Move `current` to the new toolchain when the updated channel is the current default.
6. Keep the old toolchain unless the user runs `mudup uninstall` or a cleanup command.

Versioned toolchains such as `v20260514.1144` should not change after installation.
Channel aliases such as `stable` can advance.

## Linux Shared Library Policy

Linux release artifacts should not bundle glibc files such as:

```text
libc.so.6
libm.so.6
ld-linux-x86-64.so.2
```

These are part of the target operating system and are tightly coupled with the system
loader, name service, locale, and other runtime behavior.

For dependencies such as `liburing.so.2`, choose one explicit policy per artifact:

1. Declare it as a system requirement and check for it before activation.
2. Statically link it into `mudud` if the build supports that reliably.
3. Bundle it under the toolchain `lib/` directory and set the binary runtime path to use
   the local library directory.

The first Linux release should use the simplest policy:

```text
glibc is provided by the operating system
liburing.so.2 is required on the target system
```

For `.deb` or `.rpm` packages, express this as a package dependency such as `liburing2`.
For tarball installs, `mudup` should print a clear error if `liburing.so.2` is missing.

To maximize compatibility, build Linux GNU artifacts on the oldest supported Linux
distribution. A binary built on a newer glibc may not run on older distributions.

## Security

The minimum verification requirement is SHA-256 from the channel manifest. For public
releases, add signed manifests or signed artifacts before declaring the installer stable.

Recommended approach:

- Ship a public signing key with `mudup`.
- Sign channel manifests.
- Verify the manifest signature before trusting artifact URLs and checksums.
- Verify artifact SHA-256 after download.
- Extract archives defensively: reject absolute paths, `..` path traversal, symlinks that
  escape the extraction directory, and unexpected file ownership or mode bits.

## Failure Handling

`mudup` should prefer explicit errors over partial repair.

Required behavior:

- Interrupted downloads can be retried.
- Failed verification deletes the downloaded artifact.
- Failed extraction deletes the temporary extraction directory.
- Existing toolchains are never overwritten in place.
- Default toolchain changes happen only after the new toolchain is fully installed.

## Bootstrap

`mudup-init` should be a small standalone installer script or binary. It should:

1. Detect the host platform.
2. Download the `mudup` binary for that platform.
3. Verify it.
4. Install it into `${HOME}/.mudup/bin/mudup`.
5. Run `mudup install stable`.
6. Print shell `PATH` instructions when needed.

The bootstrap installer should remain simple. Most logic belongs in `mudup` itself so it
can be updated and tested like a normal Rust binary.
