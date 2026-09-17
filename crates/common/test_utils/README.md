# test_utils

Test-only utilities for generating arbitrary data with the [`arbitrary`](https://crates.io/crates/arbitrary) crate.

## Responsibility

- Provide bounded arbitrary generators for common test inputs (names, strings, generic collections).
- Define shared limits used by arbitrary test data generation.
- Offer helper functions for repeatedly producing `Arbitrary` values from raw bytes.

## What does NOT belong here

- Concrete test cases for a specific crate (those live in the consuming crate's `tests/` or `#[cfg(test)]` modules).
- Fuzzing infrastructure or harness setup (belongs in a dedicated fuzzing crate).
- Domain-specific data generators that depend on MuduDB types (belong in the crates that define those types).

## Main public entry points

- `_arb_limit` — constants that cap the size of arbitrary keys, values, names, strings, and arrays.
- `_arb_name` — `_arbitrary_name` for generating bounded ASCII names.
- `_arb_string` — `_arbitrary_string` for generating strings up to a given length.
- `_arbitrary` — `_arbitrary_data` and `_arbitrary_vec_n` for generic collection generation.
- `add` — placeholder helper exposed by the crate root.
