use crate::data_type::DataType;
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_type_impl::data_type_create::create_object_type;
use crate::data_type_impl::fn_object::fn_object_out;
use crate::data_value::DataValue;
use crate::type_error::TyErr;
use crate::type_family::TypeFamily;
use arbitrary::Arbitrary;
use arbitrary::Unstructured;

const OBJECT_FIELD_TYPE_IDS: [TypeFamily; 9] = [
    TypeFamily::I32,
    TypeFamily::I64,
    TypeFamily::F32,
    TypeFamily::F64,
    TypeFamily::String,
    TypeFamily::U128,
    TypeFamily::I128,
    TypeFamily::Binary,
    TypeFamily::Array,
];

fn arbitrary_name(u: &mut Unstructured, prefix: &str, index: usize) -> arbitrary::Result<String> {
    let len = (u8::arbitrary(u)? as usize % 8) + 1;
    let mut s = String::with_capacity(prefix.len() + len + 8);
    s.push_str(prefix);
    s.push('_');
    s.push_str(&index.to_string());
    s.push('_');
    for _ in 0..len {
        let ch = (u8::arbitrary(u)? % 26) + b'a';
        s.push(ch as char);
    }
    Ok(s)
}

fn arbitrary_field_type(u: &mut Unstructured) -> arbitrary::Result<DataType> {
    let index = (u8::arbitrary(u)? as usize) % OBJECT_FIELD_TYPE_IDS.len();
    let type_id = OBJECT_FIELD_TYPE_IDS[index];
    if type_id.has_param() {
        type_id.fn_arb_param()(u)
    } else {
        Ok(DataType::default_for(type_id))
    }
}

fn to_arb_err(e: TyErr) -> arbitrary::Error {
    let _ = e;
    arbitrary::Error::IncorrectFormat
}

pub fn fn_object_arb_typed(
    u: &mut Unstructured,
    data_type: &DataType,
) -> arbitrary::Result<DataValue> {
    let param = data_type.expect_record_param();
    let mut fields = Vec::with_capacity(param.fields().len());
    for (_, field_ty) in param.fields() {
        let value = field_ty.type_family().fn_arb_internal()(u, field_ty)?;
        fields.push(value);
    }
    Ok(DataValue::from_record(fields))
}

pub fn fn_object_arb_printable(
    u: &mut Unstructured,
    data_type: &DataType,
) -> arbitrary::Result<String> {
    let value = fn_object_arb_typed(u, data_type)?;
    let textual = fn_object_out(&value, data_type).map_err(to_arb_err)?;
    Ok(textual.into())
}

pub fn fn_object_arb_data_type_param(u: &mut Unstructured) -> arbitrary::Result<DataType> {
    let field_count = (u8::arbitrary(u)? as usize % 4) + 1;
    let name = arbitrary_name(u, "record", 0)?;
    let mut fields = Vec::with_capacity(field_count);
    for idx in 0..field_count {
        let field_name = arbitrary_name(u, "field", idx)?;
        let field_ty = arbitrary_field_type(u)?;
        fields.push((field_name, field_ty));
    }
    Ok(create_object_type(name, fields))
}

pub const FN_OBJECT_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_object_arb_data_type_param,
    value_object: fn_object_arb_typed,
    value_print: fn_object_arb_printable,
};
