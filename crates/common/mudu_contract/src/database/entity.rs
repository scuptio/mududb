//! `database::entity` module.
//!
//! An [`Entity`] is a typed row: a Rust struct mapped positionally onto a
//! tuple described by a [`TupleFieldDesc`]. The trait carries the row mapping
//! only; SQL text and column metadata live on the generated structs, and the
//! transport layer (`mudu_query` / `mudu_command`) stays outside this layer.
#![allow(missing_docs)]

use crate::tuple::tuple_field::TupleField;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use crate::tuple::tuple_value::TupleValue;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_type as data_type;
use mudu_type::data_binary::DataBinary;
use mudu_type::data_textual::DataTextual;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::datum::{Datum, DatumDyn};
use mudu_type::type_family::TypeFamily;

pub trait Entity: Datum {
    /// Tuple layout of the row: column names, types, and declaration order.
    /// This description drives the tuple binary storage format.
    fn tuple_desc() -> &'static TupleFieldDesc;

    /// Name of the underlying database table.
    fn table_name() -> &'static str;

    /// Decodes a row from per-field binary values; `None` is a SQL NULL.
    fn from_tuple(row: &TupleField) -> RS<Self>;

    /// Decodes a row from per-field memory values (used by `EntitySet`).
    fn from_tuple_value(row: &TupleValue) -> RS<Self>;

    /// Encodes the row into per-field binary values; `None` is a SQL NULL.
    fn to_tuple(&self) -> RS<TupleField>;

    /// Encodes the row into per-field memory values.
    fn to_tuple_value(&self) -> RS<TupleValue>;
}

/// Validates the field count of a decoded row against the entity layout.
pub fn expect_field_count(actual: usize, expected: usize) -> RS<()> {
    if actual != expected {
        return Err(mudu_error!(
            ErrorCode::InvalidType,
            format!("entity row has {actual} fields, expected {expected}")
        ));
    }
    Ok(())
}

fn null_in_not_null_column(table_name: &str, column: &str) -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::InvalidType,
        format!("NULL value in NOT NULL column {table_name}.{column}")
    )
}

/// Decodes a `NOT NULL` field from its tuple binary; a NULL is an error.
pub fn field_from_tuple_binary<T: Datum>(
    table_name: &str,
    column: &str,
    binary: &Option<Vec<u8>>,
) -> RS<T> {
    match binary {
        Some(binary) => T::from_binary(binary),
        None => Err(null_in_not_null_column(table_name, column)),
    }
}

/// Decodes a nullable field from its tuple binary; NULL maps to `None`.
pub fn opt_field_from_tuple_binary<T: Datum>(binary: &Option<Vec<u8>>) -> RS<Option<T>> {
    binary
        .as_ref()
        .map(|binary| T::from_binary(binary))
        .transpose()
}

/// Decodes a `NOT NULL` field from its memory value; a NULL is an error.
pub fn field_from_tuple_value<T: Datum>(
    table_name: &str,
    column: &str,
    value: &DataValue,
) -> RS<T> {
    if value.is_null() {
        return Err(null_in_not_null_column(table_name, column));
    }
    T::from_value(value)
}

/// Decodes a nullable field from its memory value; NULL maps to `None`.
pub fn opt_field_from_tuple_value<T: Datum>(value: &DataValue) -> RS<Option<T>> {
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(T::from_value(value)?))
}

/// Encodes a `NOT NULL` field into its tuple binary.
pub fn field_to_tuple_binary<T: Datum>(value: &T) -> RS<Option<Vec<u8>>> {
    Ok(Some(value.to_binary(&T::data_type())?.into()))
}

/// Encodes a nullable field into its tuple binary; `None` maps to NULL.
pub fn opt_field_to_tuple_binary<T: Datum>(value: &Option<T>) -> RS<Option<Vec<u8>>> {
    value
        .as_ref()
        .map(|value| Ok(value.to_binary(&T::data_type())?.into()))
        .transpose()
}

/// Encodes a `NOT NULL` field into its memory value.
pub fn field_to_tuple_value<T: Datum>(value: &T) -> RS<DataValue> {
    value.to_value(&T::data_type())
}

/// Encodes a nullable field into its memory value; `None` maps to NULL.
pub fn opt_field_to_tuple_value<T: Datum>(value: &Option<T>) -> RS<DataValue> {
    match value {
        Some(value) => value.to_value(&T::data_type()),
        None => Ok(DataValue::null()),
    }
}

/// Record [`DataType`] of an entity, built from its tuple description.
pub fn entity_data_type<E: Entity>() -> DataType {
    let table_name = E::table_name().to_string();
    let field_desc = E::tuple_desc();
    let mut vec = Vec::new();
    for field in field_desc.fields() {
        let data_type = field.data_type();
        vec.push((field.name().to_string(), data_type.clone()));
    }
    data_type::record::new_record_type(table_name, vec)
}

pub fn entity_type_family() -> RS<TypeFamily> {
    Ok(TypeFamily::Record)
}

/// Decodes an entity from a record memory value.
pub fn entity_from_value<E: Entity>(value: &DataValue) -> RS<E> {
    let Some(record) = value.as_record() else {
        return Err(mudu_error!(
            ErrorCode::InvalidType,
            "expected a record value"
        ));
    };
    E::from_tuple_value(&TupleValue::from(record.clone()))
}

/// Decodes an entity from its record binary encoding.
pub fn entity_from_binary<E: Entity>(binary: &[u8]) -> RS<E> {
    let ty = E::data_type();
    let (value, _) = ty.type_family().fn_recv()(binary, &ty).map_err(|e| {
        mudu_error!(
            ErrorCode::TypeConversionFailed,
            "convert binary to entity error",
            e
        )
    })?;
    entity_from_value(&value)
}

/// Decodes an entity from its record textual encoding.
pub fn entity_from_textual<E: Entity>(textual: &str) -> RS<E> {
    let ty = E::data_type();
    let value = ty.type_family().fn_input()(textual, &ty).map_err(|e| {
        mudu_error!(
            ErrorCode::TypeConversionFailed,
            "input from string error",
            e
        )
    })?;
    entity_from_value(&value)
}

/// Encodes an entity into a record memory value.
pub fn entity_to_value<E: Entity>(entity: &E, ty: &DataType) -> RS<DataValue> {
    if ty.type_family() != TypeFamily::Record {
        return Err(mudu_error!(
            ErrorCode::TypeConversionFailed,
            "convert entity to a non-record type is not supported"
        ));
    }
    let values = entity.to_tuple_value()?.into();
    Ok(DataValue::from_record(values))
}

/// Encodes an entity into its record binary encoding.
pub fn entity_to_binary<E: Entity>(entity: &E, ty: &DataType) -> RS<DataBinary> {
    let value = entity_to_value(entity, ty)?;
    let binary = ty.type_family().fn_send()(&value, ty).map_err(|e| {
        mudu_error!(
            ErrorCode::TypeConversionFailed,
            "convert to binary error",
            e
        )
    })?;
    Ok(binary)
}

/// Encodes an entity into its record textual encoding.
pub fn entity_to_textual<E: Entity>(entity: &E, ty: &DataType) -> RS<DataTextual> {
    let value = entity_to_value(entity, ty)?;
    let textual = ty.type_family().fn_output()(&value, ty).map_err(|e| {
        mudu_error!(
            ErrorCode::TypeConversionFailed,
            "convert to textual error",
            e
        )
    })?;
    Ok(textual)
}

pub fn entity_clone_boxed<E: Entity>(entity: &E) -> Box<dyn DatumDyn> {
    Box::new(entity.clone())
}
