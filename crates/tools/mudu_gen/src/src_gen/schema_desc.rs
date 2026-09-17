//! Build a machine-readable `syscall_schema` descriptor ([`UniSchemaDesc`])
//! from parsed WIT definitions.
//!
//! The descriptor is the contract consumed by the descriptor-driven generic
//! codec runtimes of each language. It carries the protobuf-style 1-based
//! field numbers (WIT declaration order), variant wire tags, enum numbers and
//! per-function `message_kind` ordinals; it does not change the wire format
//! itself.

use crate::src_gen::wit_def::WitDef;
use mudu_binding::universal::uni_def::RecordField;
use mudu_binding::universal::uni_schema_desc::{
    UniSchemaDesc, UniSchemaEnum, UniSchemaEnumCase, UniSchemaField, UniSchemaFunc,
    UniSchemaRecord, UniSchemaTable, UniSchemaVariant, UniSchemaVariantCase,
};

/// Build an aggregated schema descriptor from parsed WIT files.
///
/// The result is deterministic regardless of input order: records, tables,
/// variants and enums are sorted by name, functions by (`message_kind`, name)
/// — within one file `message_kind` values are unique, so declaration order
/// is preserved.
pub fn build_schema_desc(sources: &[WitDef]) -> UniSchemaDesc {
    let mut desc = UniSchemaDesc::default();
    for wit_dat in sources {
        for record in &wit_dat.records {
            desc.records.push(UniSchemaRecord {
                name: record.record_name.clone(),
                fields: to_schema_fields(&record.record_fields),
            });
        }
        for table in &wit_dat.tables {
            desc.tables.push(UniSchemaTable {
                name: table.table_name.clone(),
                key: to_schema_fields(&table.table_key),
                value: to_schema_fields(&table.table_value),
            });
        }
        for variant in &wit_dat.variants {
            desc.variants.push(UniSchemaVariant {
                name: variant.variant_name.clone(),
                cases: variant
                    .variant_cases
                    .iter()
                    .enumerate()
                    .map(|(i, case)| UniSchemaVariantCase {
                        tag: i as u32,
                        name: case.vc_case_name.clone(),
                        payload_type: case.vc_case_type.clone(),
                    })
                    .collect(),
            });
        }
        for enum_def in &wit_dat.enums {
            desc.enums.push(UniSchemaEnum {
                name: enum_def.enum_name.clone(),
                cases: enum_def
                    .enum_cases
                    .iter()
                    .map(|case| UniSchemaEnumCase {
                        number: case.ec_number,
                        name: case.ec_name.clone(),
                    })
                    .collect(),
            });
        }
        for (i, func) in wit_dat.functions.iter().enumerate() {
            desc.functions.push(UniSchemaFunc {
                name: func.func_name.clone(),
                // Same rule the func-codec generator applies: the MSSP
                // `MessageKind` discriminant is the 1-based WIT declaration
                // ordinal within the file.
                message_kind: (i + 1) as u32,
                params: to_schema_fields(&func.params),
                results: to_schema_fields(&func.returns),
            });
        }
    }
    desc.records.sort_by(|a, b| a.name.cmp(&b.name));
    desc.tables.sort_by(|a, b| a.name.cmp(&b.name));
    desc.variants.sort_by(|a, b| a.name.cmp(&b.name));
    desc.enums.sort_by(|a, b| a.name.cmp(&b.name));
    desc.functions.sort_by(|a, b| {
        a.message_kind
            .cmp(&b.message_kind)
            .then(a.name.cmp(&b.name))
    });
    desc
}

fn to_schema_fields(fields: &[RecordField]) -> Vec<UniSchemaField> {
    fields
        .iter()
        .enumerate()
        .map(|(i, field)| UniSchemaField {
            // The WIT parser assigns `rf_number` after parsing; fall back to
            // the positional number for hand-built definitions.
            number: if field.rf_number == 0 {
                (i + 1) as u32
            } else {
                field.rf_number
            },
            name: field.rf_name.clone(),
            data_type: field.rf_type.clone(),
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::build_schema_desc;
    use crate::src_gen::wit_parser::WitParser;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_scalar::UniScalar;

    fn parse(text: &str) -> crate::src_gen::wit_def::WitDef {
        WitParser::new().parse_text(text).unwrap()
    }

    // Miri cannot execute FFI calls into the tree-sitter C parser.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn builds_records_variants_enums_and_functions() {
        let wit = parse(
            r#"
            package test:api;
            interface binding {
                record point { x: s32, y: s32 }
                table kv { key: { id: u64 }, value: { data: string } }
                variant shape {
                    empty,
                    circle(f32),
                    rect(tuple<f32, f32>),
                }
                enum color { red, green, blue }
                get: func(oid: u64, key: blob) -> result<option<blob>, string>;
            }
            "#,
        );
        let desc = build_schema_desc(&[wit]);

        assert_eq!(desc.records.len(), 1);
        assert_eq!(desc.records[0].name, "point");
        let numbers: Vec<u32> = desc.records[0].fields.iter().map(|f| f.number).collect();
        assert_eq!(numbers, vec![1, 2]);
        assert_eq!(desc.records[0].fields[0].name, "x");
        assert!(matches!(
            desc.records[0].fields[0].data_type,
            UniDataType::Scalar(UniScalar::I32)
        ));

        assert_eq!(desc.tables.len(), 1);
        assert_eq!(desc.tables[0].key[0].number, 1);
        assert_eq!(desc.tables[0].value[0].number, 1);
        assert_eq!(desc.tables[0].value[0].name, "data");

        assert_eq!(desc.variants.len(), 1);
        let tags: Vec<u32> = desc.variants[0].cases.iter().map(|c| c.tag).collect();
        assert_eq!(tags, vec![0, 1, 2]);
        assert!(desc.variants[0].cases[0].payload_type.is_none());
        assert!(matches!(
            desc.variants[0].cases[1].payload_type,
            Some(UniDataType::Scalar(UniScalar::F32))
        ));
        assert!(matches!(
            desc.variants[0].cases[2].payload_type,
            Some(UniDataType::Tuple(_))
        ));

        assert_eq!(desc.enums.len(), 1);
        let enum_numbers: Vec<u32> = desc.enums[0].cases.iter().map(|c| c.number).collect();
        assert_eq!(enum_numbers, vec![0, 1, 2]);
        assert_eq!(desc.enums[0].cases[2].name, "blue");

        assert_eq!(desc.functions.len(), 1);
        let func = &desc.functions[0];
        assert_eq!(func.name, "get");
        assert_eq!(func.message_kind, 1);
        let param_numbers: Vec<u32> = func.params.iter().map(|p| p.number).collect();
        assert_eq!(param_numbers, vec![1, 2]);
        assert_eq!(func.params[0].name, "oid");
        assert!(matches!(
            func.params[1].data_type,
            UniDataType::Scalar(UniScalar::Blob)
        ));
        assert_eq!(func.results.len(), 1);
        assert!(matches!(func.results[0].data_type, UniDataType::Result(_)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn message_kind_is_one_based_declaration_order_per_file() {
        let wit = parse(
            r#"
            package test:api;
            interface binding {
                first: func() -> result<_, string>;
                second: func() -> result<_, string>;
                third: func() -> result<_, string>;
            }
            "#,
        );
        let desc = build_schema_desc(&[wit]);
        let kinds: Vec<u32> = desc.functions.iter().map(|f| f.message_kind).collect();
        assert_eq!(kinds, vec![1, 2, 3]);
        let names: Vec<&str> = desc.functions.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["first", "second", "third"]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn aggregation_is_sorted_regardless_of_input_order() {
        let a = parse(
            r#"
            package test:a;
            interface binding { record alpha { v: u8 } }
            "#,
        );
        let b = parse(
            r#"
            package test:b;
            interface binding { record beta { v: u8 } }
            "#,
        );
        let desc1 = build_schema_desc(&[a.clone(), b.clone()]);
        let desc2 = build_schema_desc(&[b, a]);
        let names1: Vec<&str> = desc1.records.iter().map(|r| r.name.as_str()).collect();
        let names2: Vec<&str> = desc2.records.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names1, vec!["alpha", "beta"]);
        assert_eq!(names1, names2);
    }
}
