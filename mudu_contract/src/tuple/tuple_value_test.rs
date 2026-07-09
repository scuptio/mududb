#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::tuple_value::TupleValue;
    use mudu_type::data_value::DataValue;

    #[test]
    fn tuple_value_from_values_and_accessors() {
        let value = TupleValue::from(vec![DataValue::from_i32(1), DataValue::from_i64(2)]);
        assert_eq!(value.values().len(), 2);
        assert_eq!(*value.as_ref().values()[0].as_i32().unwrap(), 1);
    }

    #[test]
    fn tuple_value_into_consumes_inner_vec() {
        let inner = vec![DataValue::from_i32(42)];
        let value = TupleValue::from(inner.clone());
        let extracted: Vec<DataValue> = value.into();
        assert_eq!(extracted.len(), 1);
        assert_eq!(*extracted[0].as_i32().unwrap(), 42);
    }
}
