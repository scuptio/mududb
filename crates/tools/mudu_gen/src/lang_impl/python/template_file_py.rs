//! Askama template wrapping all blocks in a Python source file.

use askama::Template;

/// Askama template wrapping all blocks in a Python source file.
#[derive(Template)]
#[template(path = "python/file.py.jinja", escape = "none")]
pub struct TemplateFilePy {
    /// File-level metadata.
    pub file: FileInfoPy,
}

/// Metadata for a generated Python file.
pub struct FileInfoPy {
    /// Import statements of sibling generated modules.
    pub using_stmts: Vec<String>,
    /// Rendered code blocks.
    pub blocks: Vec<String>,
}
