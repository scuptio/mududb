# mpm_install

A small npm-style command-line installer for MuduDB `.mpk` packages. The
installed binary is named `mpm-install`.

## Usage

```bash
# Install a local .mpk package into a MuduDB server
mpm-install ./wallet.mpk

# Use the short alias
mpm-install i ./wallet.mpk

# Specify the server on the command line
mpm-install --server 192.168.1.100:8300 install ./wallet.mpk

# Use a custom config file
mpm-install --cfg ./mpm.cfg install ./wallet.mpk
```

## Configuration

`mpm-install` looks for configuration in the following order:

1. The file given with `--cfg`
2. `./mpm.cfg` in the current directory
3. `~/.mududb/mpm.cfg`

Example `mpm.cfg`:

```toml
server = "127.0.0.1:8300"
package = "./wallet.mpk"
```

Command-line flags override values from the config file.

## How it works

`mpm-install` reads the local `.mpk` archive and sends it to the MuduDB HTTP
management endpoint `POST /mudu/app/install`, reusing the same install logic
as `mcli app-install`.
