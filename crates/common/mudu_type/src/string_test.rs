#[cfg(test)]
mod tests {
    use crate::data_type::DataType;
    use crate::data_type_impl::data_type_create;
    use crate::data_type_param_string::DataTypeParamString;
    use crate::string::new_array_type;
    use crate::type_family::TypeFamily;

    #[test]
    fn new_array_type_creates_string_type_with_length() {
        let t = new_array_type(Some(10));
        assert_eq!(t.type_family(), TypeFamily::String);
        assert_eq!(t.as_string_param().unwrap().length(), 10);

        let expected = data_type_create::create_string_type(Some(10));
        assert_eq!(t.type_family(), expected.type_family());
        assert_eq!(
            t.as_string_param().unwrap().length(),
            expected.as_string_param().unwrap().length()
        );
    }

    #[test]
    fn new_array_type_creates_unbounded_string_type() {
        let t = new_array_type(None);
        assert_eq!(t.type_family(), TypeFamily::String);
        assert_eq!(t.as_string_param().unwrap().length(), 0);

        let expected = DataType::from_string(DataTypeParamString::new(0));
        assert_eq!(t.type_family(), expected.type_family());
        assert_eq!(
            t.as_string_param().unwrap().length(),
            expected.as_string_param().unwrap().length()
        );
    }
}
