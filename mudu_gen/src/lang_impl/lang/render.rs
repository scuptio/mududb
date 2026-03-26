use crate::lang_impl::lang::abstract_template::AbstractTemplate;
use mudu::common::result::RS;

pub trait Render {
    fn render(&self, template: AbstractTemplate) -> RS<String>;
}
