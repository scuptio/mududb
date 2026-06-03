# MuduDB Language Bindings

This directory is the proposed home for generated bindings and thin language wrappers.

Rules:

- Wasm/component bindings should derive from `wit/`.
- Native bindings should remain explicitly separated from component bindings.
- Language wrappers should stay thin and must not reimplement core database logic.
