use crate::lang_impl::lang::abstract_template::AbstractTemplate;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::template_kind::TemplateKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::create_render::create_render;
use crate::src_gen::wit_parser::WitParser;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::record::record_def::RecordDef;
use mudu_binding::universal::uni_dat_type::UniDatType;
use mudu_binding::universal::uni_type_desc::UniTypeDesc;
use sql_parser::parser::ddl_parser::DDLParser;
use std::collections::HashMap;

pub struct CodeGen {

}


pub struct GenResult {
    /// use defined record type
    /// `key`: record name
    /// `value`: UniDatType struct
    pub used_defined_record_type: UniTypeDesc,

    /// source code
    /// `key`: file name(stem without extension)
    /// `value`: source content text
    pub source_code: HashMap<String, String>,
}

impl GenResult {
    fn new() -> Self {
        Self {
            used_defined_record_type: Default::default(),
            source_code: Default::default(),
        }
    }

    pub fn extend(&mut self, other: Self) {
        self.used_defined_record_type.extend(other.used_defined_record_type);
        self.source_code.extend(other.source_code);
    }
}

impl Default for GenResult {
    fn default() -> Self {
        Self::new()
    }
}
impl CodeGen {
    pub fn new() -> Self {
        Self{}
    }

    pub fn extension_of_lang(lang:&str) -> RS<String> {
        let lang = LangKind::from_str(lang).unwrap();
        Ok(lang.extension().to_string())
    }

    fn from_lang(lang:&str) -> RS<LangKind> {
        let lang_kind = LangKind::from_str(lang)
            .map_or_else(|| {
                return Err(m_error!(EC::DecodeErr, format!("unknown language {}", lang)))
            }, |lang|{
                return Ok(lang)
            })?;
        Ok(lang_kind)
    }



    pub fn generate_message_code_from_wit(text:&str, lang:&str, namespace:Option<String>) -> RS<String> {
        let lang_kind = Self::from_lang(lang)?;
        Self::_generate_message(text, &lang_kind, &namespace)
    }

    pub fn generate_entity_code_from_ddl_sql(
        text: &str,
        lang: &str,
        gen_ty_def: bool,
    ) -> RS<GenResult> {
        let lang_kind = Self::from_lang(lang)?;
        Self::_generate_from_sql(text, &lang_kind, gen_ty_def)
    }

    #[allow(unused)]
    fn inline_schema_field_type(schema: &mut Vec<RecordDef>, uni_type: &mut Vec<UniDatType>) -> RS<()> {
        if schema.len() != uni_type.len() {
            return Err(m_error!(EC::InternalErr, format!("table schema and its record type length mismatch {} != {}", schema.len(), uni_type.len())))
        }
        let n = schema.len();
        for i in 0..n {
            let schema = &mut schema[i];
            let uni_type = &uni_type[i];
            schema.update_field_inline(uni_type)?;
        }
        Ok(())
    }

    fn _generate_record_type(
        record_list: &Vec<RecordDef>,
        ty_def: &mut UniTypeDesc,
    ) -> RS<()> {
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
                    return Err(m_error!(EC::DBInternalError, format!("expected a record type, {:?}", ty)))
                }
            }
        }
        Ok(())
    }

    fn _generate_from_sql(
        text: &str,
        lang: &LangKind,
        gen_ty_def: bool,
    ) -> RS<GenResult> {
        let ml_parser = DDLParser::new();
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

    fn _generate_message(text: &str, lang: &LangKind, namespace:&Option<String>) -> RS<String> {
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
                } else {
                    if template.namespace != interface_name {
                        return Err(m_error!(EC::ParseErr, "expected at most one interface"))
                    }
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
        let render = create_render(lang);
        let source_code = render.render(template)?;
        Ok(source_code)
    }
}


#[cfg(test)]
mod test {
    use crate::src_gen::code_gen::CodeGen;
    use mudu::this_file;
    use rust_format::{Formatter, RustFmt};
    use std::env::temp_dir;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test() {
        let path = PathBuf::from(this_file!());
        let path = path.parent().unwrap().to_path_buf()
            .join("contract.wit");
        let str = fs::read_to_string(path).unwrap();
        let src_code = CodeGen::generate_message_code_from_wit(&str, "rust", None).unwrap();
        let src_code = RustFmt::default().format_str(src_code).unwrap();
        let tmp_path = temp_dir().join("interface.rs");
        fs::write(tmp_path, src_code).unwrap();
    }
}