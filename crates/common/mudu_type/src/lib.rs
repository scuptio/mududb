//! The mududb data type system: type descriptors, runtime values, and the
//! per-type conversion, comparison, and parameter function tables.
#![warn(missing_docs)]

/// Array type construction and array values.
pub mod array;
/// Binary datum buffer wrapper.
pub mod data_binary;
#[cfg(test)]
mod data_binary_test;
/// Textual datum string wrapper.
pub mod data_textual;
/// The `DataType` concrete type descriptor.
pub mod data_type;
#[cfg(any(test, feature = "arbitrary"))]
mod data_type_fn_arbitrary;
/// Comparison function pointers of data types.
pub mod data_type_fn_compare;
#[cfg(test)]
mod data_type_fn_compare_test;
/// Builders that attach a type to a datum.
pub mod data_typed;
#[cfg(test)]
mod data_typed_test;
/// The `DataValue` runtime value.
pub mod data_value;
#[cfg(test)]
mod data_value_array_test;
/// Datum access traits bridging typed and untyped values.
pub mod datum;
#[cfg(test)]
mod datum_test;
/// The `TypeFamily` implementation-family identifier.
pub mod type_family;

/// Conversion function pointers of data types.
pub mod data_type_fn_convert;
/// Parameter parsing function pointers of data types.
pub mod data_type_fn_param;
#[cfg(test)]
mod data_type_fn_param_test;
mod data_type_impl;
/// The serializable `DataTypeInfo` type descriptor.
pub mod data_type_info;
/// The `DataTypeParam` trait for type parameters.
pub mod data_type_param;
/// The `LenKind` length classification of data types.
pub mod len_kind;
#[cfg(test)]
mod len_kind_test;
mod type_kind;

/// Shared type parameter name constants.
pub mod param;

mod data_json;
/// MessagePack datum wrapper.
pub mod data_msg_pack;
#[cfg(test)]
mod data_msg_pack_test;

/// The `DataTypeFunction` per-type function table trait.
pub mod data_type_function;
#[cfg(test)]
mod data_type_function_test;
/// The `DataValueInner` value representation.
pub mod data_value_inner;

/// The `DataTypeParamArray` array type parameter.
pub mod data_type_param_array;
#[cfg(test)]
mod data_type_param_array_test;

/// The `DataTypeParamKind` type parameter enum.
pub mod data_type_param_kind;
#[cfg(test)]
mod data_type_param_kind_test;

/// The `DataTypeParamNumeric` numeric type parameter.
pub mod data_type_param_numeric;
/// The `DataTypeParamRecord` record type parameter.
pub mod data_type_param_record;
#[cfg(test)]
mod data_type_param_record_test;

/// The `DataTypeParamString` string type parameter.
pub mod data_type_param_string;
#[cfg(test)]
mod data_type_param_string_test;

/// The `DataTypeParamTime` time type parameter.
pub mod data_type_param_time;
#[cfg(test)]
mod data_type_param_time_test;

/// The `DataTypeParamTimestamp` timestamp type parameter.
pub mod data_type_param_timestamp;
#[cfg(test)]
mod data_type_param_timestamp_test;

/// The `DataTypeParamTimestampTz` timestamp-with-timezone type parameter.
pub mod data_type_param_timestamptz;
#[cfg(test)]
mod data_type_param_timestamptz_test;

/// Record type construction.
pub mod record;
/// The `ScalarType` scalar type descriptor.
pub mod scalar_type;

#[cfg(test)]
mod scalar_type_test;

/// String type construction.
pub mod string;
#[cfg(test)]
mod string_test;

/// Type system error types.
pub mod type_error;
#[cfg(test)]
mod type_error_test;
//pub mod universal;
