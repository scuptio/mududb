use askama::Template;

/// Askama template wrapping all blocks in a C# source file.
#[derive(Template)]
#[template(path = "csharp/file.cs.jinja", escape = "none")]
pub struct TemplateFileCS {
    /// File-level metadata.
    pub file: FileInfo,
}

/// Metadata for a generated C# file.
pub struct FileInfo {
    /// Namespace of the file.
    pub namespace: String,
    /// Using/import statements.
    pub using_stmts: Vec<String>,
    /// Rendered code blocks.
    pub blocks: Vec<String>,
}
