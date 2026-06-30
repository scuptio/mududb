#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::tuple::build_tuple::build_tuple;
    use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
    use crate::tuple::tuple_ref::TupleRef;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;

    #[test]
    fn tuple_ref_reads_fixed_len_field() {
        let desc = TupleBinaryDesc::from(vec![DatType::new_no_param(DatTypeID::I32)]).unwrap();
        let bytes: Vec<u8> = vec![0, 0, 0, 42];
        let tuple = build_tuple(std::slice::from_ref(&bytes), &desc).unwrap();
        let tuple_ref = TupleRef::new(&tuple, &desc);
        assert_eq!(tuple_ref.columns(), 1);
        assert_eq!(tuple_ref.get_tuple(), tuple.as_slice());
        let data = tuple_ref.get_binary_data(0).unwrap();
        assert_eq!(data, bytes.as_slice());
    }

    #[test]
    fn tuple_ref_new_and_columns() {
        let desc = TupleBinaryDesc::from(vec![
            DatType::new_no_param(DatTypeID::I32),
            DatType::new_no_param(DatTypeID::I64),
        ])
        .unwrap();
        let tuple = build_tuple(&[vec![0; 4], vec![0; 8]], &desc).unwrap();
        let tuple_ref = TupleRef::new(&tuple, &desc);
        assert_eq!(tuple_ref.columns(), 2);
    }

    #[test]
    fn tuple_ref_reads_var_len_field() {
        let desc = TupleBinaryDesc::from(vec![DatType::default_for(DatTypeID::String)]).unwrap();
        let bytes = b"hello".to_vec();
        let tuple = build_tuple(std::slice::from_ref(&bytes), &desc).unwrap();
        let tuple_ref = TupleRef::new(&tuple, &desc);
        let data = tuple_ref.get_binary_data(0).unwrap();
        assert_eq!(data, bytes.as_slice());
    }
}
