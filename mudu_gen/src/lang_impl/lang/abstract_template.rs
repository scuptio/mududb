use crate::lang_impl::lang::template_kind::TemplateKind;

pub struct AbstractTemplate {
    pub namespace: String,
    pub using_stmts: Vec<Vec<String>>,
    pub elements: Vec<TemplateKind>,
}

impl AbstractTemplate {
    pub fn new() -> AbstractTemplate {
        Self {
            namespace: "".to_string(),
            using_stmts: vec![],
            elements: vec![],
        }
    }
}