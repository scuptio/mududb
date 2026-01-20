use mudu::common::result::RS;
use mudu::utils::json::JsonValue;
use crate::tuple::datum_desc::DatumDesc;

pub fn tuple_binary_to_json(binary: &[u8], desc: &DatumDesc) -> RS<JsonValue> {
    let obj = desc.dat_type();
    let param = obj;
    let tp_id = desc.dat_type_id();
    let (dat_internal, _) = tp_id.fn_recv()(binary, param)
        .map_err(|e| e.to_m_err())?;
    let dat_printable = tp_id.fn_output_json()(&dat_internal, param)
        .map_err(|e| e.to_m_err())?;
    let value = dat_printable.into_json_value();
    Ok(value)
}