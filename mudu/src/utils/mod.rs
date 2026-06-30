//! Pure utility helpers that do not perform I/O.

/// Fixed-size binary length prefix.
pub mod bin_size;
#[cfg(test)]
mod bin_size_test;

/// Fixed-size binary offset/length pair.
pub mod bin_slot;
#[cfg(test)]
mod bin_slot_test;

/// Length-prefixed buffer helpers.
pub mod buf;
#[cfg(test)]
mod buf_test;

/// Case-conversion helpers.
pub mod case_convert;

#[cfg(test)]
mod case_convert_test;

/// JSON serialization helpers.
pub mod json;
#[cfg(test)]
mod json_test;

/// MessagePack serialization helpers.
pub mod msg_pack;

/// TOML serialization helpers.
pub mod toml;
#[cfg(test)]
mod toml_test;
