#[cfg(test)]
mod tests {
    use crate::universal::uni_dat_type::UniDatType;
    use crate::universal::uni_def::{EnumCase, RecordField, UniRecordDef, UniTableDef};
    use crate::universal::uni_scalar::UniScalar;

    fn sample_field() -> RecordField {
        RecordField {
            rf_comments: "c".to_string(),
            rf_name: "f".to_string(),
            rf_type: UniDatType::Scalar(UniScalar::I32),
        }
    }

    #[test]
    fn uni_record_def_display_contains_name() {
        let def = UniRecordDef {
            record_comments: "comment".to_string(),
            record_name: "R".to_string(),
            record_fields: vec![sample_field()],
        };
        let s = def.to_string();
        assert!(s.contains("UniRecordDef"));
        assert!(s.contains("R"));
    }

    #[test]
    fn uni_table_def_display_contains_name() {
        let def = UniTableDef {
            table_comments: "comment".to_string(),
            table_name: "T".to_string(),
            table_key: vec![sample_field()],
            table_value: vec![sample_field()],
        };
        let s = def.to_string();
        assert!(s.contains("UniTableDef"));
        assert!(s.contains("T"));
    }

    #[test]
    fn enum_case_fields_are_accessible() {
        let case = EnumCase {
            ec_comments: "cc".to_string(),
            ec_name: "A".to_string(),
            ec_number: 1,
        };
        assert_eq!(case.ec_name, "A");
        assert_eq!(case.ec_number, 1);
    }
}
