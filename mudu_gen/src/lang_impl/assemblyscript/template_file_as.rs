use askama::Template;

/// Askama template wrapping all blocks in an AssemblyScript source file.
#[derive(Template)]
#[template(path = "assemblyscript/file.ts.jinja", escape = "none")]
pub struct TemplateFileAS {
    /// File-level metadata.
    pub file: FileInfo,
}

/// Metadata for a generated AssemblyScript file.
pub struct FileInfo {
    /// Import statements.
    pub using_stmts: Vec<String>,
    /// Rendered code blocks.
    pub blocks: Vec<String>,
}
