#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use crate::contract::schema_column::SchemaColumn;
    use crate::contract::schema_table::SchemaTable;
    use crate::contract::table_info::TableInfo;
    use crate::sql::copy_layout::CopyLayout;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dt_info::DTInfo;

    fn table_desc() -> std::sync::Arc<crate::contract::table_desc::TableDesc> {
        let schema = SchemaTable::new(
            "accounts".to_string(),
            vec![
                SchemaColumn::new(
                    "tenant_id".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
                ),
                SchemaColumn::new(
                    "user_id".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
                ),
                SchemaColumn::new(
                    "name".to_string(),
                    DatTypeID::String,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::String)),
                ),
            ],
            vec![0, 1],
            vec![2],
        );
        TableInfo::new(schema).unwrap().table_desc().unwrap()
    }

    #[test]
    fn empty_columns_use_schema_order() {
        let table = table_desc();
        let layout = CopyLayout::new(&table, &[]).unwrap();
        assert_eq!(layout.key_index(), &[0, 1]);
        assert_eq!(layout.value_index(), &[2]);
    }

    #[test]
    fn full_column_list_reorders_key_and_value_positions() {
        let table = table_desc();
        let columns = vec![
            "user_id".to_string(),
            "name".to_string(),
            "tenant_id".to_string(),
        ];
        let layout = CopyLayout::new(&table, &columns).unwrap();
        assert_eq!(layout.key_index(), &[2, 0]);
        assert_eq!(layout.value_index(), &[1]);
    }

    #[test]
    fn copy_layout_rejects_partial_column_list() {
        let table = table_desc();
        let columns = vec!["tenant_id".to_string(), "user_id".to_string()];
        let err = match CopyLayout::new(&table, &columns) {
            Ok(_) => panic!("expected partial column list error"),
            Err(err) => err,
        };
        assert!(err
            .to_string()
            .contains("is not equal to the size specified"));
    }

    #[test]
    fn copy_layout_rejects_missing_named_column() {
        let table = table_desc();
        let columns = vec![
            "tenant_id".to_string(),
            "user_id".to_string(),
            "missing".to_string(),
        ];
        let err = match CopyLayout::new(&table, &columns) {
            Ok(_) => panic!("expected missing column error"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("cannot find column name name"));
    }
}
