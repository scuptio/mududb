#[cfg(test)]
pub mod _fuzz {
    use crate::contract::schema_table::SchemaTable;
    use arbitrary::{Arbitrary, Unstructured};

    #[cfg(test)]
    pub fn _schema_table(data: &[u8]) {
        let mut u = Unstructured::new(data);
        let mut vec = vec![];
        while !u.is_empty() {
            let r = SchemaTable::arbitrary(&mut u);
            let s = match r {
                Ok(s) => s,
                Err(_) => break,
            };
            vec.push(s);
        }

        for s in vec.iter() {
            let (key_desc, key_mapping) = s.key_tuple_desc();
            let (value_desc, value_mapping) = s.value_tuple_desc();
            let key_columns = s.key_columns();
            let value_columns = s.value_columns();
            for (_i, (columns, desc, mapping)) in vec![
                (key_columns, key_desc, key_mapping),
                (value_columns, value_desc, value_mapping),
            ]
                .into_iter()
                .enumerate()
            {
                assert_eq!(desc.field_count(), mapping.len());
                for (i, field_info) in mapping.iter().enumerate() {
                    let fd = desc.get_field_desc(i);
                    let sc = &columns[field_info.column_index()];
                    if _i == 0 {
                        assert!(sc.is_primary())
                    } else if _i == 1 {
                        assert!(!sc.is_primary())
                    }
                    assert_eq!(sc.get_index(), field_info.column_index() as u32);
                    assert_eq!(sc.is_fixed_length(), fd.is_fixed_len());
                    assert_eq!(sc.type_id(), fd.data_type());
                    assert_eq!(sc.get_name(), field_info.name());
                }
                let (value_desc, value_mapping) = s.value_tuple_desc();
                assert_eq!(value_desc.field_count(), value_mapping.len());
            }
        }

        for sch in vec.iter() {
            let _sch1 = sch.clone();
            let _str1 = format!("{:?}", _sch1);
            let json_str = serde_json::to_string(sch).unwrap();
            let _sch2: SchemaTable = serde_json::from_str(&json_str).unwrap();
            let _str2 = format!("{:?}", _sch2);
            assert_eq!(_str1, _str2);
            assert_eq!(_sch2.id(), sch.id());
            assert_eq!(_sch2.table_name(), sch.table_name());
        }
    }
}

#[cfg(test)]
mod _test {
    use crate::fuzz::_test_target::_test::_test_target;

    #[test]
    fn test_schema_table() {
        _test_target("_schema_table");
    }
}
