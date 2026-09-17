use crate::lang_impl::lang::template_kind::TemplateKind;

/// Language-agnostic representation of a file to render.
pub struct AbstractTemplate {
    /// Namespace or package name.
    pub namespace: String,
    /// Imported/use paths.
    pub using_stmts: Vec<Vec<String>>,
    /// Top-level definitions to render.
    pub elements: Vec<TemplateKind>,
}

impl AbstractTemplate {
    /// Create an empty template.
    pub fn new() -> AbstractTemplate {
        Self {
            namespace: "".to_string(),
            using_stmts: vec![],
            elements: vec![],
        }
    }
}
