use askama::Template;

#[derive(Template)]
#[template(path = "csharp/file.cs.jinja", escape = "none")]
pub struct TemplateFileCS {
    pub file: FileInfo,
}

pub struct FileInfo {
    pub namespace: String,
    pub using_stmts: Vec<String>,
    pub blocks : Vec<String>
}