#[cfg(test)]
mod tests {
    use crate::universal::uni_data_type::UniDataType;
    use crate::universal::uni_def::{
        EnumCase, StructField, UniClassDef, UniStructDef, assign_field_numbers,
    };
    use crate::universal::uni_scalar::UniScalar;

    fn sample_field() -> StructField {
        StructField::new(
            "c".to_string(),
            "f".to_string(),
            UniDataType::Scalar(UniScalar::I32),
        )
    }

    #[test]
    fn assign_field_numbers_is_one_based_declaration_order() {
        let mut fields = vec![sample_field(), sample_field(), sample_field()];
        assign_field_numbers(&mut fields);
        let numbers: Vec<u32> = fields.iter().map(|f| f.rf_number).collect();
        assert_eq!(numbers, vec![1, 2, 3]);
    }

    #[test]
    fn uni_record_def_display_contains_name() {
        let def = UniStructDef {
            struct_comments: "comment".to_string(),
            struct_name: "R".to_string(),
            struct_fields: vec![sample_field()],
        };
        let s = def.to_string();
        assert!(s.contains("UniStructDef"));
        assert!(s.contains("R"));
    }

    #[test]
    fn uni_class_def_display_contains_name() {
        let def = UniClassDef {
            table_comments: "comment".to_string(),
            class_name: "T".to_string(),
            table_key: vec![sample_field()],
            table_value: vec![sample_field()],
        };
        let s = def.to_string();
        assert!(s.contains("UniClassDef"));
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
