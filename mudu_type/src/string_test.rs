#[cfg(test)]
mod tests {
    use crate::dat_type::DatType;
    use crate::dat_type_id::DatTypeID;
    use crate::dt_impl::dt_create;
    use crate::dtp_string::DTPString;
    use crate::string::new_array_type;

    #[test]
    fn new_array_type_creates_string_type_with_length() {
        let t = new_array_type(Some(10));
        assert_eq!(t.dat_type_id(), DatTypeID::String);
        assert_eq!(t.as_string_param().unwrap().length(), 10);

        let expected = dt_create::create_string_type(Some(10));
        assert_eq!(t.dat_type_id(), expected.dat_type_id());
        assert_eq!(
            t.as_string_param().unwrap().length(),
            expected.as_string_param().unwrap().length()
        );
    }

    #[test]
    fn new_array_type_creates_unbounded_string_type() {
        let t = new_array_type(None);
        assert_eq!(t.dat_type_id(), DatTypeID::String);
        assert_eq!(t.as_string_param().unwrap().length(), 0);

        let expected = DatType::from_string(DTPString::new(0));
        assert_eq!(t.dat_type_id(), expected.dat_type_id());
        assert_eq!(
            t.as_string_param().unwrap().length(),
            expected.as_string_param().unwrap().length()
        );
    }
}
