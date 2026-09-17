use crate::lang_impl::assemblyscript::as_codec::{
    AS_STYLE_RECORD, AsTupleHolder, CollectingTupleNamer, as_codec_default, as_codec_type,
    as_decode_stmts, as_encode_stmts, as_join,
};
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::record_info::RecordInfo;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_def::UniRecordDef;

/// Precomputed explicit-mpack codec statements for one record field.
pub struct AsFieldCodec {
    /// 1-based field number (the wire map key).
    pub number: u32,
    /// AssemblyScript field type.
    pub ty: String,
    /// Default-value expression.
    pub default: String,
    /// Encode statements (joined, indented for the method body).
    pub encode_stmts: String,
    /// Decode statements (joined, indented for the `case` block body).
    pub decode_stmts: String,
    /// Whether the field is a WIT `option<T>`; omitted from the encoded
    /// map when null.
    pub is_option: bool,
}

/// Askama template for an AssemblyScript record.
#[derive(Template)]
#[template(path = "assemblyscript/record.ts.jinja", escape = "none")]
pub struct TemplateRecordAS {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Normalized record metadata.
    pub record: RecordInfo,
    /// Per-field codec statements, parallel to `record.record_fields`.
    pub field_codecs: Vec<AsFieldCodec>,
    /// Tuple holder classes referenced by tuple-typed fields.
    pub tuple_holders: Vec<AsTupleHolder>,
    /// Whether any field is a WIT `option<T>`; the encoder only emits the
    /// omit-when-absent guarded form then (option-less records keep the
    /// historical fixed-header shape).
    pub has_option_field: bool,
}

impl TemplateRecordAS {
    /// Build the template from a WIT record definition.
    pub fn from(record_def: UniRecordDef, cfg: CodegenCfg) -> RS<Self> {
        let record = RecordInfo::from(
            record_def.clone(),
            LangKind::AssemblyScript,
            &cfg.type_kinds,
        )?;
        let mut namer = CollectingTupleNamer::with_kinds(cfg.type_kinds.clone());
        let mut field_codecs = Vec::with_capacity(record.record_fields.len());
        for (field, field_def) in record
            .record_fields
            .iter()
            .zip(record_def.record_fields.iter())
        {
            let access = format!("value.{}", field.rf_name);
            let base = format!("{}{}", record.record_name, to_pascal_case(&field.rf_name));
            namer.begin(base.clone());
            let ty = as_codec_type(&field_def.rf_type, &mut namer)?;
            let default = as_codec_default(&field_def.rf_type, &mut namer)?;
            let mut enc = Vec::new();
            as_encode_stmts(AS_STYLE_RECORD, &field_def.rf_type, &access, 8, &mut enc)?;
            namer.begin(base);
            let mut dec = Vec::new();
            as_decode_stmts(
                AS_STYLE_RECORD,
                &field_def.rf_type,
                &access,
                16,
                &mut dec,
                &mut namer,
            )?;
            field_codecs.push(AsFieldCodec {
                number: field.rf_number,
                ty,
                default,
                encode_stmts: as_join(&enc),
                decode_stmts: as_join(&dec),
                is_option: matches!(
                    field_def.rf_type,
                    mudu_binding::universal::uni_data_type::UniDataType::Option(_)
                ),
            });
        }
        let tuple_holders = namer.holders().to_vec();
        let has_option_field = field_codecs.iter().any(|codec| codec.is_option);
        Ok(Self {
            cfg,
            record,
            field_codecs,
            tuple_holders,
            has_option_field,
        })
    }
}
