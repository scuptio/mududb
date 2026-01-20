use askama::Template;

#[derive(Template)]
#[template(path = "rust/mudu_proc.rs.jinja", escape = "none")]
pub struct TemplateProc {
    pub procedure: ProcedureInfo,
}

pub struct ArgumentInfo {
    pub arg_name: String,
    pub arg_type: String,
    pub arg_index: usize,
}

pub struct ReturnInfo {
    pub ret_type: String,
    #[allow(unused)]
    pub ret_index: usize,
}

pub struct ProcedureInfo {
    pub mod_name: String,
    pub fn_name: String,
    pub wit_fn_exported_name: String,
    pub wit_async_true: String,
    pub fn_exported_name: String,
    pub fn_inner_name: String,
    pub guest_struct_name: String,
    pub fn_argv_desc: String,
    pub fn_result_desc: String,
    pub fn_proc_desc: String,
    pub package_name: String,
    pub argument_list: Vec<ArgumentInfo>,
    pub return_tuple: Vec<ReturnInfo>,
    pub opt_async: String,
    pub opt_dot_await: String,
    pub opt_underline_async: String,
}
