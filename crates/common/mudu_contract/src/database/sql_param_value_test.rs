#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::database::sql_param_value::SQLParamValue;
    use crate::database::sql_params::SQLParams;
    use mudu_type::data_value::DataValue;

    #[test]
    fn sql_param_value_empty() {
        let params = SQLParamValue::from_vec(vec![]);
        assert_eq!(params.size(), 0);
        assert!(params.params().is_empty());
        assert!(params.get_idx(0).is_none());
        assert!(params.into().is_empty());
    }

    #[test]
    fn sql_param_value_access() {
        let params = SQLParamValue::from_vec(vec![
            DataValue::from_i32(42),
            DataValue::from_string("hello".to_string()),
        ]);
        assert_eq!(params.size(), 2);
        assert_eq!(params.params().len(), 2);

        let first = params.get_idx(0).unwrap();
        assert_eq!(
            first.type_family().unwrap(),
            mudu_type::type_family::TypeFamily::I32
        );

        let second = params.get_idx(1).unwrap();
        assert_eq!(
            second.type_family().unwrap(),
            mudu_type::type_family::TypeFamily::String
        );

        assert!(params.get_idx(2).is_none());

        let owned = params.into();
        assert_eq!(owned.len(), 2);
        assert_eq!(*owned[0].as_i32().unwrap(), 42);
    }
}
