//! Askama template data for a C record (`typedef struct` plus
//! `static inline` MessagePack codecs).
//!
//! The template is dumb: every statement it emits is precomputed here (see
//! [`crate::lang_impl::c::c_codec`]) so the logic stays unit-testable. The
//! generated record encodes as a MessagePack map keyed by the 1-based field
//! numbers; WIT `option<T>` fields carry a `<name>_is_null` flag next to the
//! value and are omitted from the map when null (proto3 presence semantics).

use crate::lang_impl::c::c_codec::{
    C_STYLE_RECORD, CMapEntry, CollectingCNamer, c_decode_stmts, c_encode_stmts, c_ident, c_join,
    c_map_decode_loop, c_type,
};
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::record_info::RecordInfo;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_snake_case;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_def::UniRecordDef;

/// Precomputed rendering data for one record field.
pub struct CField {
    /// Field doc comments (`//` lines; empty when undocumented).
    pub comments: String,
    /// Sanitized snake-case field name.
    pub name: String,
    /// C field type (the inner type for `option<T>` fields).
    pub ty: String,
    /// Whether the field is a WIT `option<T>`; a `<name>_is_null` flag is
    /// generated next to the value and the field is omitted from the encoded
    /// map when null.
    pub is_option: bool,
}

/// Askama template for a C record struct (holders plus the struct itself).
#[derive(Template)]
#[template(path = "c/record.h.jinja", escape = "none")]
pub struct TemplateRecordC {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Record doc comments (`//` lines; empty when undocumented).
    pub comments: String,
    /// Snake-case record name (`uni_oid`).
    pub name: String,
    /// Precomputed composite holder typedefs referenced by the fields.
    pub holder_decls: Vec<String>,
    /// Record fields in declaration order.
    pub fields: Vec<CField>,
    /// Encode function body (indented for the function).
    pub encode_body: String,
    /// Decode function body (indented for the function).
    pub decode_body: String,
}

/// Askama template for a C record's codec functions (encode/decode plus the
/// arena-allocating `<type>_decode_new`), borrowing the struct data.
#[derive(Template)]
#[template(path = "c/record_codec.h.jinja", escape = "none")]
pub struct TemplateRecordCodecC<'a> {
    /// The struct template data built by [`TemplateRecordC::from`].
    pub data: &'a TemplateRecordC,
}

impl TemplateRecordC {
    /// Build the template from a WIT record definition.
    pub fn from(record_def: UniRecordDef, cfg: CodegenCfg) -> RS<Self> {
        let record = RecordInfo::from(record_def.clone(), LangKind::C, &cfg.type_kinds)?;
        let name = to_snake_case(&record.record_name);
        let mut namer = CollectingCNamer::new();
        let mut fields = Vec::with_capacity(record.record_fields.len());
        let mut enc_lines: Vec<String> = Vec::new();
        let mut map_entries: Vec<CMapEntry> = Vec::new();
        let mut option_inits: Vec<String> = Vec::new();
        for (field, field_def) in record
            .record_fields
            .iter()
            .zip(record_def.record_fields.iter())
        {
            let field_name = c_ident(&to_snake_case(&field.rf_name));
            let base = format!("{}_{}", name, field_name);
            let is_option = matches!(field_def.rf_type, UniDataType::Option(_));
            // The value type of an option field is its inner type; presence
            // rides on the `<name>_is_null` flag.
            let value_ty = match &field_def.rf_type {
                UniDataType::Option(inner) => inner.as_ref().clone(),
                other => other.clone(),
            };
            namer.begin(base.clone());
            let ty = c_type(&value_ty, &mut namer)?;
            let access = format!("value->{}", field_name);
            let enc_indent = if is_option { 8 } else { 4 };
            let mut enc = Vec::new();
            c_encode_stmts(C_STYLE_RECORD, &value_ty, &access, enc_indent, 0, &mut enc)?;
            let key_write = format!(
                "{}mpw_u64(w, {}u);",
                " ".repeat(enc_indent),
                field.rf_number
            );
            if is_option {
                enc_lines.push(format!("    if (!value->{}_is_null) {{", field_name));
                enc_lines.push(key_write);
                enc_lines.extend(enc);
                enc_lines.push("    }".to_string());
                option_inits.push(format!("    out->{}_is_null = 1;", field_name));
            } else {
                enc_lines.push(key_write);
                enc_lines.extend(enc);
            }
            namer.begin(base);
            let target = format!("out->{}", field_name);
            let mut dec = Vec::new();
            if is_option {
                dec.push("if (mpr_try_nil(r)) {".to_string());
                dec.push(format!("    {}_is_null = 1;", target));
                dec.push("} else {".to_string());
                dec.push(format!("    {}_is_null = 0;", target));
                c_decode_stmts(
                    C_STYLE_RECORD,
                    &value_ty,
                    &target,
                    4,
                    0,
                    &mut dec,
                    &mut namer,
                )?;
                dec.push("}".to_string());
            } else {
                c_decode_stmts(
                    C_STYLE_RECORD,
                    &value_ty,
                    &target,
                    0,
                    0,
                    &mut dec,
                    &mut namer,
                )?;
            }
            // Re-indent the branch body for the map loop (indent + 16).
            let dec = dec
                .iter()
                .map(|line| {
                    if line.is_empty() {
                        line.clone()
                    } else {
                        format!("                    {}", line)
                    }
                })
                .collect::<Vec<_>>();
            map_entries.push(CMapEntry {
                number: field.rf_number,
                decode_stmts: c_join(&dec),
            });
            fields.push(CField {
                comments: field.rf_comments.clone(),
                name: field_name,
                ty,
                is_option,
            });
        }

        let map_header = map_header_expr(fields.len(), &fields);
        let mut encode_body = vec![format!("    mpw_map_header(w, {});", map_header)];
        if fields.is_empty() {
            encode_body.push("    (void)value;".to_string());
        }
        encode_body.extend(enc_lines);

        let mut decode_body = vec![
            "    (void)a;".to_string(),
            "    memset(out, 0, sizeof(*out));".to_string(),
        ];
        decode_body.extend(option_inits);
        decode_body.push(c_map_decode_loop(&map_entries, 4));
        decode_body.push("    return mpr_ok(r) ? 0 : -1;".to_string());

        let holder_decls = namer.holders().iter().map(|h| h.decl()).collect();
        Ok(Self {
            cfg,
            comments: record.record_comments,
            name,
            holder_decls,
            fields,
            encode_body: c_join(&encode_body),
            decode_body: c_join(&decode_body),
        })
    }
}

/// The encoded-map-count expression: the field count minus the option
/// fields, plus one per present option field (proto3 presence semantics).
fn map_header_expr(field_count: usize, fields: &[CField]) -> String {
    let mut expr = format!("{}u", field_count);
    for field in fields.iter().filter(|f| f.is_option) {
        expr.push_str(&format!(
            " - 1u + (value->{}_is_null ? 0u : 1u)",
            field.name
        ));
    }
    expr
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{TemplateRecordC, TemplateRecordCodecC};
    use crate::src_gen::codegen_cfg::CodegenCfg;
    use askama::Template;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_def::{RecordField, UniRecordDef};
    use mudu_binding::universal::uni_scalar::UniScalar;

    fn field(name: &str, ty: UniDataType) -> RecordField {
        RecordField::new(String::new(), name.to_string(), ty)
    }

    fn render(def: UniRecordDef) -> RS<(String, String)> {
        let template = TemplateRecordC::from(def, CodegenCfg::new())?;
        let struct_part = template.render().unwrap();
        let codec_part = (TemplateRecordCodecC { data: &template }).render().unwrap();
        Ok((struct_part, codec_part))
    }

    #[test]
    fn renders_oid_record() -> RS<()> {
        let def = UniRecordDef {
            record_comments: "// object id".to_string(),
            record_name: "uni-oid".to_string(),
            record_fields: vec![
                field("h", UniDataType::Scalar(UniScalar::U64)),
                field("l", UniDataType::Scalar(UniScalar::U64)),
            ],
        };
        let (struct_part, codec_part) = render(def)?;
        assert!(struct_part.contains("struct uni_oid {"));
        assert!(struct_part.contains("uint64_t h;"));
        assert!(
            codec_part
                .contains("MP_INLINE void uni_oid_encode(mp_writer *w, const uni_oid *value) {")
        );
        assert!(codec_part.contains("mpw_map_header(w, 2u);"));
        assert!(codec_part.contains("mpw_u64(w, (uint64_t)(value->h));"));
        assert!(
            codec_part.contains(
                "MP_INLINE int uni_oid_decode(mp_reader *r, mp_arena *a, uni_oid *out) {"
            )
        );
        assert!(codec_part.contains("out->h = mpr_u64(r);"));
        assert!(codec_part.contains("return mpr_ok(r) ? 0 : -1;"));
        assert!(
            codec_part
                .contains("MP_INLINE uni_oid *uni_oid_decode_new(mp_reader *r, mp_arena *a) {")
        );
        Ok(())
    }

    #[test]
    fn option_fields_carry_is_null_and_are_omitted_when_null() -> RS<()> {
        let def = UniRecordDef {
            record_comments: String::new(),
            record_name: "uni-query-argv".to_string(),
            record_fields: vec![
                field("oid", UniDataType::Identifier("uni-oid".to_string())),
                field(
                    "param-desc",
                    UniDataType::Option(Box::new(UniDataType::Identifier(
                        "uni-record-type".to_string(),
                    ))),
                ),
            ],
        };
        let (struct_part, codec_part) = render(def)?;
        assert!(struct_part.contains("uni_record_type param_desc;"));
        assert!(struct_part.contains("bool param_desc_is_null;"));
        assert!(
            codec_part
                .contains("mpw_map_header(w, 2u - 1u + (value->param_desc_is_null ? 0u : 1u));")
        );
        assert!(codec_part.contains("if (!value->param_desc_is_null) {"));
        assert!(codec_part.contains("uni_record_type_encode(w, &(value->param_desc));"));
        assert!(codec_part.contains("out->param_desc_is_null = 1;"));
        assert!(codec_part.contains("if (mpr_try_nil(r)) {"));
        Ok(())
    }

    #[test]
    fn list_fields_use_named_slice_types() -> RS<()> {
        let def = UniRecordDef {
            record_comments: String::new(),
            record_name: "uni-sql-param".to_string(),
            record_fields: vec![field(
                "params",
                UniDataType::Array(Box::new(UniDataType::Identifier(
                    "uni-data-value".to_string(),
                ))),
            )],
        };
        let (struct_part, codec_part) = render(def)?;
        assert!(struct_part.contains(
            "typedef struct { uni_data_value *items; uint32_t len; } uni_sql_param_params_list;"
        ));
        assert!(struct_part.contains("uni_sql_param_params_list params;"));
        assert!(codec_part.contains("mpw_array_header(w, (value->params).len);"));
        assert!(codec_part.contains("uni_data_value_encode(w, &((value->params).items[i0]));"));
        assert!(codec_part.contains("uni_data_value *p0 = (uni_data_value *)mpa_alloc("));
        Ok(())
    }

    #[test]
    fn keyword_field_names_are_sanitized() -> RS<()> {
        let def = UniRecordDef {
            record_comments: String::new(),
            record_name: "kw".to_string(),
            record_fields: vec![field("union", UniDataType::Scalar(UniScalar::U32))],
        };
        let (struct_part, _) = render(def)?;
        assert!(struct_part.contains("uint32_t union_;"));
        Ok(())
    }
}
