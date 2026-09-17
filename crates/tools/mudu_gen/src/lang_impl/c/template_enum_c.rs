//! Askama template data for a C enum (`typedef enum` plus `static inline`
//! MessagePack codecs; the wire form is the case number as u64, decoded
//! leniently without a range check, matching the host and the other
//! bindings).

use crate::lang_impl::c::c_codec::{c_const_name, c_type_name};
use crate::lang_impl::lang::enum_info::EnumInfo;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu_binding::universal::uni_def::UniEnumDef;

/// One case of a generated C enum.
pub struct CEnumCase {
    /// Case doc comments (`//` lines; empty when undocumented).
    pub comments: String,
    /// Enumerator constant (`UNI_SCALAR_U8`).
    pub const_name: String,
    /// Numeric discriminator.
    pub number: u32,
}

/// Askama template for a C enum.
#[derive(Template)]
#[template(path = "c/enum.h.jinja", escape = "none")]
pub struct TemplateEnumC {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Enum doc comments (`//` lines; empty when undocumented).
    pub comments: String,
    /// Snake-case enum name (`uni_scalar`).
    pub name: String,
    /// Enum cases in declaration order.
    pub cases: Vec<CEnumCase>,
}

impl TemplateEnumC {
    /// Build the template from a WIT enum definition.
    pub fn from(enum_def: UniEnumDef, cfg: CodegenCfg) -> RS<Self> {
        let info = EnumInfo::from(enum_def, LangKind::C)?;
        let prefix = c_const_name(&info.enum_name);
        let mut cases = Vec::with_capacity(info.enum_cases.len());
        for case in &info.enum_cases {
            cases.push(CEnumCase {
                comments: case.ec_comments.clone(),
                const_name: format!("{}_{}", prefix, c_const_name(&case.ec_name)),
                number: case.ec_number,
            });
        }
        Ok(Self {
            cfg,
            comments: info.enum_comments.clone(),
            name: c_type_name(&info.enum_name),
            cases,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::TemplateEnumC;
    use crate::src_gen::codegen_cfg::CodegenCfg;
    use askama::Template;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_def::{EnumCase, UniEnumDef};

    #[test]
    fn renders_enum_with_codecs() -> RS<()> {
        let def = UniEnumDef {
            enum_comments: String::new(),
            enum_name: "uni-scalar".to_string(),
            enum_cases: vec![
                EnumCase {
                    ec_comments: String::new(),
                    ec_name: "bool".to_string(),
                    ec_number: 0,
                },
                EnumCase {
                    ec_comments: String::new(),
                    ec_name: "%u8".to_string(),
                    ec_number: 1,
                },
            ],
        };
        let template = TemplateEnumC::from(def, CodegenCfg::new())?;
        let out = template.render().unwrap();
        assert!(out.contains("UNI_SCALAR_BOOL = 0u,"));
        assert!(out.contains("UNI_SCALAR_U8 = 1u,"));
        assert!(out.contains("} uni_scalar;"));
        assert!(
            out.contains(
                "MP_INLINE void uni_scalar_encode(mp_writer *w, const uni_scalar *value) {"
            )
        );
        assert!(out.contains("mpw_u64(w, (uint64_t)*value);"));
        assert!(out.contains("*out = (uni_scalar)mpr_u64(r);"));
        Ok(())
    }
}
