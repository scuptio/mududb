//! Machine-readable syscall schema descriptor (`syscall_schema`).
//!
//! [`UniSchemaDesc`] describes every type and function of a set of WIT files
//! in a form the descriptor-driven generic codec runtimes (Rust / C# /
//! AssemblyScript / host) can consume. Field numbers are the protobuf-style
//! 1-based ordinals of the WIT declaration order; variant tags and enum
//! numbers are the 0-based wire ordinals; a function's `message_kind` is its
//! 1-based ordinal in WIT declaration order within its file.

use crate::universal::uni_data_type::UniDataType;
use serde::{Deserialize, Serialize};

/// A collection of schema descriptions for WIT records, tables, variants,
/// enums and functions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UniSchemaDesc {
    pub records: Vec<UniSchemaRecord>,
    pub tables: Vec<UniSchemaTable>,
    pub variants: Vec<UniSchemaVariant>,
    pub enums: Vec<UniSchemaEnum>,
    pub functions: Vec<UniSchemaFunc>,
}

/// A record (or a table key/value body) field: number, name and type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniSchemaField {
    /// 1-based field number in declaration order.
    pub number: u32,
    pub name: String,
    pub data_type: UniDataType,
}

/// A WIT record definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniSchemaRecord {
    pub name: String,
    pub fields: Vec<UniSchemaField>,
}

/// A WIT table definition; key and value bodies are encoded like records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniSchemaTable {
    pub name: String,
    pub key: Vec<UniSchemaField>,
    pub value: Vec<UniSchemaField>,
}

/// A WIT variant definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniSchemaVariant {
    pub name: String,
    pub cases: Vec<UniSchemaVariantCase>,
}

/// A variant case: wire tag and optional payload type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniSchemaVariantCase {
    /// 0-based wire tag in declaration order.
    pub tag: u32,
    pub name: String,
    pub payload_type: Option<UniDataType>,
}

/// A WIT enum definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniSchemaEnum {
    pub name: String,
    pub cases: Vec<UniSchemaEnumCase>,
}

/// An enum case: 0-based number and name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniSchemaEnumCase {
    pub number: u32,
    pub name: String,
}

/// A WIT function declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniSchemaFunc {
    pub name: String,
    /// The MSSP `MessageKind` discriminant: 1-based ordinal in WIT
    /// declaration order within the declaring file.
    pub message_kind: u32,
    /// Parameters; `number` is the 1-based parameter ordinal in declaration
    /// order.
    pub params: Vec<UniSchemaField>,
    /// Named return values (empty or a single unnamed result type for MSSP).
    pub results: Vec<UniSchemaField>,
}
