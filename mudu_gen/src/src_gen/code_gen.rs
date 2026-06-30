//! High-level code generation API.

use crate::lang_impl::lang::abstract_template::AbstractTemplate;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::template_kind::TemplateKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::create_render::create_render;
use crate::src_gen::wit_parser::WitParser;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::record::record_def::RecordDef;
use mudu_binding::universal::uni_dat_type::UniDatType;
use mudu_binding::universal::uni_type_desc::UniTypeDesc;
use sql_parser::parser::ddl_parser::DDLParser;
use std::collections::HashMap;

/// Entry point for generating source code from WIT or SQL inputs.
pub struct CodeGen {}

/// Result of a code-generation run.
pub struct GenResult {
    /// User-defined record types produced during generation.
    ///
    /// Key: record name; value: the corresponding [`UniDatType`].
    pub used_defined_record_type: UniTypeDesc,

    /// Generated source files.
    ///
    /// Key: file stem (without extension); value: source content.
    pub source_code: HashMap<String, String>,
}

impl GenResult {
    fn new() -> Self {
        Self {
            used_defined_record_type: Default::default(),
            source_code: Default::default(),
        }
    }

    /// Merge another generation result into `self`.
    pub fn extend(&mut self, other: Self) {
        self.used_defined_record_type
            .extend(other.used_defined_record_type);
        self.source_code.extend(other.source_code);
    }
}

impl Default for GenResult {
    fn default() -> Self {
        Self::new()
    }
}
impl Default for CodeGen {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeGen {
    /// Create a new [`CodeGen`].
    pub fn new() -> Self {
        Self {}
    }

    /// Return the file extension for the given language name.
    pub fn extension_of_lang(lang: &str) -> RS<String> {
        let lang = LangKind::from_name(lang).map_or_else(
            || {
                Err(mudu_error!(
                    ErrorCode::Decode,
                    format!("unknown language {}", lang)
                ))
            },
            Ok,
        )?;
        Ok(lang.extension().to_string())
    }

    fn from_lang(lang: &str) -> RS<LangKind> {
        let lang_kind = LangKind::from_name(lang).map_or_else(
            || {
                Err(mudu_error!(
                    ErrorCode::Decode,
                    format!("unknown language {}", lang)
                ))
            },
            Ok,
        )?;
        Ok(lang_kind)
    }

    /// Generate message types from a WIT text.
    pub fn generate_message_code_from_wit(
        text: &str,
        lang: &str,
        namespace: Option<String>,
    ) -> RS<String> {
        let lang_kind = Self::from_lang(lang)?;
        Self::_generate_message(text, &lang_kind, &namespace)
    }

    /// Generate entity code from DDL SQL text.
    pub fn generate_entity_code_from_ddl_sql(
        text: &str,
        lang: &str,
        gen_ty_def: bool,
    ) -> RS<GenResult> {
        let lang_kind = Self::from_lang(lang)?;
        Self::_generate_from_sql(text, &lang_kind, gen_ty_def)
    }

    fn _generate_record_type(record_list: &[RecordDef], ty_def: &mut UniTypeDesc) -> RS<()> {
        let mut vec = Vec::with_capacity(record_list.len());
        for table in record_list.iter() {
            let ty = table.to_record_type()?;
            vec.push(UniDatType::Record(ty));
        }
        let vec = UniDatType::rewrite_inline(vec)?;
        for ty in vec {
            match &ty {
                UniDatType::Record(r) => {
                    ty_def.types.insert(to_pascal_case(&r.record_name), ty);
                }
                _ => {
                    return Err(mudu_error!(
                        ErrorCode::Database,
                        format!("expected a record type, {:?}", ty)
                    ));
                }
            }
        }
        Ok(())
    }

    fn _generate_from_sql(text: &str, lang: &LangKind, gen_ty_def: bool) -> RS<GenResult> {
        let ml_parser = DDLParser::new()?;
        let mut gen_result = GenResult::default();
        let vec_table_def = ml_parser.parse(text)?;
        Self::__generate_entity(&vec_table_def, lang, &mut gen_result.source_code)?;
        if gen_ty_def {
            Self::_generate_record_type(&vec_table_def, &mut gen_result.used_defined_record_type)?
        }
        Ok(gen_result)
    }

    fn __generate_entity(
        record_def: &Vec<RecordDef>,
        lang: &LangKind,
        source_content: &mut HashMap<String, String>,
    ) -> RS<()> {
        let render = create_render(lang);
        for schema in record_def {
            let table_name = schema.table_name().clone();
            let kind = TemplateKind::Entity(schema.clone());
            let mut template = AbstractTemplate::new();
            template.elements.push(kind);
            let source = render.render(template)?;
            source_content.insert(table_name, source);
        }
        Ok(())
    }

    fn _generate_message(text: &str, lang: &LangKind, namespace: &Option<String>) -> RS<String> {
        let parser = WitParser::new();
        let mut code_gen_cfg = CodegenCfg::new();
        code_gen_cfg.impl_default = true;
        code_gen_cfg.impl_serialize = true;
        code_gen_cfg.impl_inner_func = true;
        let wit_dat = parser.parse_text(text)?;
        let mut template = AbstractTemplate::new();
        template.using_stmts = wit_dat.use_path;

        if let Some(name) = namespace {
            template.namespace = name.to_string()
        } else {
            for interface_name in wit_dat.interface {
                if template.namespace.is_empty() {
                    template.namespace = interface_name;
                } else if template.namespace != interface_name {
                    return Err(mudu_error!(
                        ErrorCode::Parse,
                        "expected at most one interface"
                    ));
                }
            }
        }

        for enum_def in wit_dat.enums {
            let kind = TemplateKind::Enum((enum_def, code_gen_cfg.clone()));
            template.elements.push(kind);
        }
        for variant_def in wit_dat.variants {
            let kind = TemplateKind::Variant((variant_def, code_gen_cfg.clone()));
            template.elements.push(kind);
        }
        for record_def in wit_dat.records {
            let kind = TemplateKind::Record((record_def, code_gen_cfg.clone()));
            template.elements.push(kind);
        }
        for table_def in wit_dat.tables {
            let kind = TemplateKind::Table((table_def, code_gen_cfg.clone()));
            template.elements.push(kind);
        }
        let render = create_render(lang);
        let source_code = render.render(template)?;
        Ok(source_code)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
mod test {
    use crate::src_gen::code_gen::{CodeGen, GenResult};
    use mudu::error::ErrorCode;
    use mudu_utils::this_file;
    use std::path::PathBuf;

    fn contract_wit() -> String {
        let path = PathBuf::from(this_file!());
        let path = path.parent().unwrap().to_path_buf().join("contract.wit");
        mudu_sys::fs::sync::sync_read_to_string(path).unwrap()
    }

    // Miri cannot execute FFI calls into the tree-sitter C parser, so skip this
    // test under Miri. Code generation is still exercised by normal `cargo test`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn test() {
        let src_code =
            CodeGen::generate_message_code_from_wit(&contract_wit(), "rust", None).unwrap();
        let syntax = syn::parse_file(&src_code).unwrap();
        let src_code = prettyplease::unparse(&syntax);
        let tmp_path = mudu_sys::env_var::temp_dir().join("interface.rs");
        mudu_sys::fs::sync::sync_write(tmp_path, src_code).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn extension_of_lang_returns_extension() {
        assert_eq!(CodeGen::extension_of_lang("rust").unwrap(), "rs");
        assert_eq!(CodeGen::extension_of_lang("csharp").unwrap(), "cs");
    }

    #[test]
    fn extension_of_lang_rejects_unknown_language() {
        let err = CodeGen::extension_of_lang("java").unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn generate_message_code_from_wit_produces_csharp() {
        let src_code =
            CodeGen::generate_message_code_from_wit(&contract_wit(), "csharp", None).unwrap();
        assert!(src_code.contains("namespace"));
        assert!(src_code.contains("class"));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn generate_entity_code_from_ddl_sql_produces_rust() {
        let sql = "CREATE TABLE t(id INT PRIMARY KEY, name TEXT);";
        let GenResult {
            source_code,
            used_defined_record_type,
        } = CodeGen::generate_entity_code_from_ddl_sql(sql, "rust", false).unwrap();
        assert!(source_code.contains_key("t"));
        assert!(used_defined_record_type.types.is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn generate_entity_code_from_ddl_sql_produces_csharp() {
        let sql = "CREATE TABLE t(id INT PRIMARY KEY, name TEXT);";
        let GenResult { source_code, .. } =
            CodeGen::generate_entity_code_from_ddl_sql(sql, "csharp", false).unwrap();
        assert!(source_code.contains_key("t"));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn generate_entity_code_from_ddl_sql_includes_type_def_when_requested() {
        let sql = "CREATE TABLE t(id INT PRIMARY KEY, name TEXT);";
        let GenResult {
            used_defined_record_type,
            ..
        } = CodeGen::generate_entity_code_from_ddl_sql(sql, "rust", true).unwrap();
        assert!(!used_defined_record_type.types.is_empty());
    }
}
