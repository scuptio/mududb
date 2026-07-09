use crate::data_textual::DataTextual;
use crate::data_type::DataType;
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_type_param_array::DataTypeParamArray;
use crate::data_value::DataValue;
use crate::type_family::TypeFamily;
use arbitrary::{Arbitrary, Unstructured};

const ARRAY_INNER_TYPE_IDS: [TypeFamily; 8] = [
    TypeFamily::I32,
    TypeFamily::I64,
    TypeFamily::F32,
    TypeFamily::F64,
    TypeFamily::String,
    TypeFamily::U128,
    TypeFamily::I128,
    TypeFamily::Binary,
];

pub fn fn_array_arb_object(
    u: &mut Unstructured,
    data_type: &DataType,
) -> arbitrary::Result<DataValue> {
    let n = u8::arbitrary(u)? as usize;
    let param = data_type.expect_array_param();
    let inner_type = param.data_type();
    let mut vec = Vec::with_capacity(n);
    for _ in 0..n {
        let dat = inner_type.type_family().fn_arb_internal()(u, inner_type)?;
        vec.push(dat);
    }
    Ok(DataValue::from_array(vec))
}

pub fn fn_array_arb_printable(
    u: &mut Unstructured,
    data_type: &DataType,
) -> arbitrary::Result<String> {
    let object = fn_array_arb_object(u, data_type)?;
    let printable: DataTextual = TypeFamily::Array.fn_output()(&object, data_type).unwrap();
    Ok(printable.into())
}

pub fn fn_array_arb_data_type_param(u: &mut Unstructured) -> arbitrary::Result<DataType> {
    let n = (u8::arbitrary(u)? as usize) % ARRAY_INNER_TYPE_IDS.len();
    let type_family = ARRAY_INNER_TYPE_IDS[n];
    let inner_type = if type_family.has_param() {
        type_family.fn_arb_param()(u)?
    } else {
        DataType::default_for(type_family)
    };
    let param = DataTypeParamArray::new(inner_type);
    let data_type = DataType::from_array(param);
    Ok(data_type)
}

pub const FN_ARRAY_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_array_arb_data_type_param,
    value_object: fn_array_arb_object,
    value_print: fn_array_arb_printable,
};
