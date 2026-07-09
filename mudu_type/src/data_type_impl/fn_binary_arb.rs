use crate::data_textual::DataTextual;
use crate::data_type::DataType;
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_value::DataValue;
use crate::type_family::TypeFamily;
use arbitrary::{Arbitrary, Unstructured};

pub fn fn_binary_arb_object(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<DataValue> {
    let n = u8::arbitrary(u)? as usize;

    let mut vec = Vec::with_capacity(n);
    for _ in 0..n {
        let v = u8::arbitrary(u)?;
        vec.push(v);
    }
    Ok(DataValue::from_binary(vec))
}

pub fn fn_binary_arb_printable(
    u: &mut Unstructured,
    data_type: &DataType,
) -> arbitrary::Result<String> {
    let object = fn_binary_arb_object(u, data_type)?;
    let printable: DataTextual = TypeFamily::Binary.fn_output()(&object, data_type).unwrap();
    Ok(printable.into())
}

pub fn fn_binary_arb_data_type_param(_: &mut Unstructured) -> arbitrary::Result<DataType> {
    let data_type = DataType::new_no_param(TypeFamily::Binary);
    Ok(data_type)
}

pub const FN_BINARY_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_binary_arb_data_type_param,
    value_object: fn_binary_arb_object,
    value_print: fn_binary_arb_printable,
};
