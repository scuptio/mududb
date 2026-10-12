//! Conversion of a tuple binary field to a JSON value.

use crate::tuple::datum_desc::DatumDesc;
use mudu::common::result::RS;
use mudu::utils::json::JsonValue;

/// Decodes `binary` as the type described by `desc` and renders it as a JSON value.
pub fn tuple_binary_to_json(binary: &[u8], desc: &DatumDesc) -> RS<JsonValue> {
    let obj = desc.data_type();
    let param = obj;
    let tp_id = desc.type_family();
    let dat_printable = tp_id.fn_recv()(binary, param)
        .and_then(|(data_internal, _)| tp_id.fn_output_json()(&data_internal, param))
        .map_err(|e| e.to_m_err())?;
    let value = dat_printable.into_json_value();
    Ok(value)
}
