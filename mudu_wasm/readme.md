# Add wasm32 target

    rustup target add wasm32-wasip1

## Notice

Do not use wasm32-unknown-unknown.
The wasm32-unknown-unknown target is not WASI capable.

When compile with wasm32-unknown-unknown, wasm-time runtime would complain error:

    unknown import: `__wbindgen_placeholder__::__wbindgen_describe

# Install cargo-make

    cargo install cargo-make

# Build wasm32 target

    cargo build --target wasm32-wasip1

If no wasm32 target, it would complain:

    error[E0463]: can't find crate for `core`


