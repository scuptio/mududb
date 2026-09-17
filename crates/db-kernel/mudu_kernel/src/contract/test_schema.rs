#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]
#[cfg(any(test, fuzzing))]
pub mod _fuzz {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use crate::contract::field_info::FieldInfo;
    use crate::contract::schema_table::SchemaTable;
    use arbitrary::{Arbitrary, Unstructured};
    use mudu::common::id::AttrIndex;
    use std::collections::HashMap;

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
            let (key_desc, key_mapping) = s.key_tuple_desc().unwrap();
            let (value_desc, value_mapping) = s.value_tuple_desc().unwrap();
            let key_indices = s.key_indices();
            let value_indices = s.value_indices();
            for (tuple_kind, (indices, desc, mapping)) in [
                (key_indices, key_desc, key_mapping),
                (value_indices, value_desc, value_mapping),
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(desc.field_count(), mapping.len());
                assert_eq!(desc.field_count(), indices.len());

                // `mapping` is ordered by the normalized binary layout, while
                // `indices` preserves the original key/value column order. Build
                // a map from the original column index back to its field info so
                // we can validate every declared column independently of layout.
                let col_to_field: HashMap<AttrIndex, &FieldInfo> = mapping
                    .iter()
                    .map(|field_info| (field_info.column_index(), field_info))
                    .collect();

                for (pos, &col_idx) in indices.iter().enumerate() {
                    let field_info = col_to_field
                        .get(&col_idx)
                        .expect("column index from indices must exist in tuple mapping");
                    let fd = desc.get_field_desc(field_info.datum_index());
                    assert_eq!(field_info.column_index(), col_idx);

                    let sc = s.column_by_index(col_idx);
                    if tuple_kind == 0 {
                        assert!(sc.is_primary());
                    } else {
                        assert!(!sc.is_primary());
                    }
                    // `pos` is the original key/value position; `get_index()` is
                    // normalized to that position during schema construction.
                    assert_eq!(sc.get_index(), pos);
                    assert_eq!(sc.is_fixed_length(), fd.is_fixed_len());
                    assert_eq!(sc.type_id(), fd.data_type());
                    assert_eq!(sc.get_name(), field_info.name());
                }
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
