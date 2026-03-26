# pull-push wasi component

This example builds a minimal WASI component under `example/pull-push`.

It exports two async functions:

- `push(message: list<u8>) -> list<u8>`
- `pull(message: list<u8>) -> list<u8>`

Current behavior:

- `push(message)` calls the key-value syscall `put`
  - it uses `message` as both the key and the value
  - on success it returns the original `message`
- `pull(message)` calls the key-value syscall `get`
  - it uses `message` as the key
  - if the key exists, it returns the stored value
  - if the key does not exist, it returns an empty byte array

## Files

- `src/lib.rs`: component WIT export and Rust implementation
- `Cargo.toml`: crate definition for the example component

## Build

1. Install the WASI Preview 2 target:

```bash
rustup target add wasm32-wasip2
```

2. Build the component:

```bash
cargo build -p pull-push --target wasm32-wasip2
```

3. The output artifact will be generated at:

```text
target/wasm32-wasip2/debug/pull_push.wasm
```

For a release build:

```bash
cargo build -p pull-push --target wasm32-wasip2 --release
```

The release artifact will be:

```text
target/wasm32-wasip2/release/pull_push.wasm
```

## Local verification

You can run a host-side compile check without the wasm target:

```bash
cargo check -p pull-push
```

To verify the actual WASI component build, use:

```bash
cargo check -p pull-push --target wasm32-wasip2
```
