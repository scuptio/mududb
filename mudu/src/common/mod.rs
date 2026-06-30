#![allow(missing_docs)]

//! Common types and helpers shared across the workspace.

#[cfg(any(test, feature = "test"))]
pub mod _arb_de_en;
#[cfg(test)]
mod _arb_de_en_test;

pub mod buf;
#[cfg(test)]
mod buf_test;

pub mod codec;
#[cfg(test)]
mod codec_test;

pub mod crc;
#[cfg(test)]
mod crc_test;

pub mod endian;
#[cfg(test)]
mod endian_test;

pub mod expected;
#[cfg(test)]
mod expected_test;

pub mod id;
#[cfg(test)]
mod id_test;

pub mod len_payload;

pub mod result;
pub mod result_of;
pub mod slice;

pub mod app_info;
pub mod cmp_equal;
pub mod cmp_order;
pub mod default_value;
pub mod into_result;
pub mod limitation;
pub mod result_from;
pub mod serde_utils;
#[cfg(test)]
mod serde_utils_test;

pub mod update_delta;
#[cfg(test)]
mod update_delta_test;

pub mod xid;

#[cfg(test)]
mod xid_test;

#[cfg(test)]
mod result_of_test;
