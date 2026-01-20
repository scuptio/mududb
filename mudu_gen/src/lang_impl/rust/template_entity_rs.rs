use crate::entity::entity_info::EntityInfo;
use crate::lang_impl::lang::lang_kind::LangKind;
// templates.rs
use askama::Template;
use mudu::common::result::RS;
use mudu_binding::record::record_def::RecordDef;

#[derive(Template)]
#[template(path = "rust/entity.rs.jinja", escape = "none")]
pub struct TemplateEntityRS {
    pub table: EntityInfo,
}

impl TemplateEntityRS {
    pub fn from_table_schema(table_schema: &RecordDef) -> RS<Self> {
        let info = EntityInfo::from_record_def(table_schema, &LangKind::Rust)?;
        Ok(Self {
            table: info,
        })
    }
}