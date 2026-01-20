use mudu::common::result::RS;
use crate::lang_impl::lang::abstract_template::AbstractTemplate;

pub trait Render {
    fn render(&self, template:AbstractTemplate) -> RS<String>;
}