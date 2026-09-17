//! `tuple::binary_to_json` module.
#![allow(missing_docs)]

use crate::tuple::datum_desc::DatumDesc;
use mudu::common::result::RS;
use mudu::utils::json::JsonValue;

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
