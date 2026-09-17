use crate::lang_impl::lang::abstract_template::AbstractTemplate;
use mudu::common::result::RS;

/// Trait implemented by language back-ends that render [`AbstractTemplate`]s.
pub trait Render {
    /// Render the template into source code.
    fn render(&self, template: AbstractTemplate) -> RS<String>;
}
