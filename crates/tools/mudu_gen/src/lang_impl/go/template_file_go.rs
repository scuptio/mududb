//! Askama template wrapping all blocks in a Go source file.

use askama::Template;

/// Askama template wrapping all blocks in a Go source file.
#[derive(Template)]
#[template(path = "go/file.go.jinja", escape = "none")]
pub struct TemplateFileGo {
    /// Go package clause name (`types` for the binding package).
    pub package: String,
    /// Import paths referenced by the rendered blocks (`fmt`, the
    /// hand-written codec runtime); empty when the file is self-contained.
    pub imports: Vec<String>,
    /// Rendered code blocks.
    pub blocks: Vec<String>,
}
