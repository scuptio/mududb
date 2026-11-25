use crate::lang_impl::lang::lang_kind::LangKind;
use crate::object::object_info::ObjectInfo;
// templates.rs
use askama::Template;
use mudu::common::result::RS;
use mudu_binding::table::class_def::ClassDef;

/// Askama template for a Rust object struct.
#[derive(Template)]
#[template(path = "rust/object.rs.jinja", escape = "none")]
pub struct TemplateObjectRS {
    /// Object metadata used by the template.
    pub table: ObjectInfo,
}

impl TemplateObjectRS {
    /// Build the template from a table schema.
    pub fn from_table_schema(table_schema: &ClassDef) -> RS<Self> {
        let info = ObjectInfo::from_struct_def(table_schema, &LangKind::Rust)?;
        Ok(Self { table: info })
    }
}
