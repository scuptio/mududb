//! An [`crate::database::object::Object`] is a typed row: a Rust struct mapped positionally onto a
//! tuple described by a [`crate::tuple::tuple_field_desc::TupleFieldDesc`]. The trait carries the row mapping
//! only; SQL text and column metadata live on the generated structs, and the
//! transport layer (`mudu_query` / `mudu_command`) stays outside this layer.

use crate::tuple::tuple_field::TupleField;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use crate::tuple::tuple_value::TupleValue;
use mudu::common::id::OID;
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
use mudu_type::type_family_ext::TypeFamilyExt;

/// A typed row: a Rust struct mapped positionally onto a tuple described by
/// a [`crate::tuple::tuple_field_desc::TupleFieldDesc`].
pub trait Object: Datum {
    /// Tuple layout of the row: column names, types, and declaration order.
    /// This description drives the tuple binary storage format.
    fn tuple_desc() -> &'static TupleFieldDesc;

    /// Name of the underlying database table.
    fn class_name() -> &'static str;

    /// The object type name, independent of the storage carrier.
    ///
    /// Defaults to [`Object::class_name`]. A transient object without a table
    /// (e.g. a `CREATE TYPE` message) overrides this with its declared type
    /// name; this is what [`object_data_type`] uses as the record type name.
    fn type_name() -> &'static str {
        Self::class_name()
    }

    /// The object idobject (OID) of this instance, if it carries one.
    ///
    /// Defaults to `None`; objects with an `OID` primary-key column override
    /// this so the four carriers (table/queue/cache/message) share one idobject.
    fn object_id(&self) -> Option<OID> {
        None
    }

    /// Decodes a row from per-field binary values; `None` is a SQL NULL.
    fn from_tuple(row: &TupleField) -> RS<Self>;

    /// Decodes a row from per-field memory values (used by `ObjectSet`).
    fn from_tuple_value(row: &TupleValue) -> RS<Self>;

    /// Encodes the row into per-field binary values; `None` is a SQL NULL.
    fn to_tuple(&self) -> RS<TupleField>;

    /// Encodes the row into per-field memory values.
    fn to_tuple_value(&self) -> RS<TupleValue>;
}

/// Validates the field count of a decoded row against the object layout.
pub fn expect_field_count(actual: usize, expected: usize) -> RS<()> {
    if actual != expected {
        return Err(mudu_error!(
            ErrorCode::InvalidType,
            format!("object row has {actual} fields, expected {expected}")
        ));
    }
    Ok(())
}

fn null_in_not_null_column(class_name: &str, column: &str) -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::InvalidType,
        format!("NULL value in NOT NULL column {class_name}.{column}")
    )
}

/// Decodes a `NOT NULL` field from its tuple binary; a NULL is an error.
pub fn field_from_tuple_binary<T: Datum>(
    class_name: &str,
    column: &str,
    binary: &Option<Vec<u8>>,
) -> RS<T> {
    match binary {
        Some(binary) => T::from_binary(binary),
        None => Err(null_in_not_null_column(class_name, column)),
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
    class_name: &str,
    column: &str,
    value: &DataValue,
) -> RS<T> {
    if value.is_null() {
        return Err(null_in_not_null_column(class_name, column));
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

/// Struct [`DataType`] of an object, built from its tuple description.
pub fn object_data_type<E: Object>() -> DataType {
    let type_name = E::type_name().to_string();
    let field_desc = E::tuple_desc();
    let mut vec = Vec::new();
    for field in field_desc.fields() {
        let data_type = field.data_type();
        vec.push((field.name().to_string(), data_type.clone()));
    }
    data_type::struct_type::new_struct_type(type_name, vec)
}

/// Returns the type family of an object (always [`TypeFamily::Struct`]).
pub fn object_type_family() -> RS<TypeFamily> {
    Ok(TypeFamily::Struct)
}

/// Decodes an object from a record memory value.
pub fn object_from_value<E: Object>(value: &DataValue) -> RS<E> {
    let Some(record) = value.as_struct() else {
        return Err(mudu_error!(
            ErrorCode::InvalidType,
            "expected a record value"
        ));
    };
    E::from_tuple_value(&TupleValue::from(record.clone()))
}

/// Decodes an object from its record binary encoding.
pub fn object_from_binary<E: Object>(binary: &[u8]) -> RS<E> {
    let ty = E::data_type();
    let (value, _) = ty.type_family().fn_recv()(binary, &ty).map_err(|e| {
        mudu_error!(
            ErrorCode::TypeConversionFailed,
            "convert binary to object error",
            e
        )
    })?;
    object_from_value(&value)
}

/// Decodes an object from its record textual encoding.
pub fn object_from_textual<E: Object>(textual: &str) -> RS<E> {
    let ty = E::data_type();
    let value = ty.type_family().fn_input()(textual, &ty).map_err(|e| {
        mudu_error!(
            ErrorCode::TypeConversionFailed,
            "input from string error",
            e
        )
    })?;
    object_from_value(&value)
}

/// Encodes an object into a record memory value.
pub fn object_to_value<E: Object>(object: &E, ty: &DataType) -> RS<DataValue> {
    if ty.type_family() != TypeFamily::Struct {
        return Err(mudu_error!(
            ErrorCode::TypeConversionFailed,
            "convert object to a non-record type is not supported"
        ));
    }
    let values = object.to_tuple_value()?.into();
    Ok(DataValue::from_struct(values))
}

/// Encodes an object into its record binary encoding.
pub fn object_to_binary<E: Object>(object: &E, ty: &DataType) -> RS<DataBinary> {
    let value = object_to_value(object, ty)?;
    let binary = ty.type_family().fn_send()(&value, ty).map_err(|e| {
        mudu_error!(
            ErrorCode::TypeConversionFailed,
            "convert to binary error",
            e
        )
    })?;
    Ok(binary)
}

/// Encodes an object into its record textual encoding.
pub fn object_to_textual<E: Object>(object: &E, ty: &DataType) -> RS<DataTextual> {
    let value = object_to_value(object, ty)?;
    let textual = ty.type_family().fn_output()(&value, ty).map_err(|e| {
        mudu_error!(
            ErrorCode::TypeConversionFailed,
            "convert to textual error",
            e
        )
    })?;
    Ok(textual)
}

/// Clones an object into a boxed datum trait object.
pub fn object_clone_boxed<E: Object>(object: &E) -> Box<dyn DatumDyn> {
    Box::new(object.clone())
}

/// Decodes an object from a row, skipping `skip` leading fields.
///
/// Queue `RECEIVE` rows carry the system columns `__seq` and `__lsn` ahead of
/// the payload columns; callers pass `skip = 2` to decode the payload as an
/// object. The remaining fields map positionally onto [`Object::from_tuple`].
pub fn object_from_row_skipping<E: Object>(row: &TupleField, skip: usize) -> RS<E> {
    let payload = TupleField::new_nullable(row.fields()[skip..].to_vec());
    E::from_tuple(&payload)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::tuple::tuple_field::TupleField;

    fn i64_field(v: i64) -> Option<Vec<u8>> {
        Some(v.to_binary(&<i64 as Datum>::data_type()).unwrap().into())
    }

    #[test]
    fn object_from_row_skipping_strips_leading_fields() {
        // A queue RECEIVE row carries [__seq, __lsn, payload]; skipping 2
        // leaves the payload column, decoded as an i64 object.
        let row = TupleField::new_nullable(vec![i64_field(7), i64_field(8), i64_field(42)]);
        let value: i64 = object_from_row_skipping(&row, 2).unwrap();
        assert_eq!(value, 42);
    }
}
