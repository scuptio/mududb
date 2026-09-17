//! Askama template wrapping all blocks of one generated C header file.

use askama::Template;

/// Askama template wrapping all blocks in a generated C header.
#[derive(Template)]
#[template(path = "c/file.h.jinja", escape = "none")]
pub struct TemplateFileC {
    /// File-level metadata.
    pub file: CFileInfo,
}

/// Metadata for a generated C header file.
pub struct CFileInfo {
    /// Include guard macro (`MUDUD_TYPES_UNI_OID_H`).
    pub guard: String,
    /// Early cross-file type includes (`UniOid` for
    /// `mududb/types/UniOid.h`): types this file needs COMPLETE before its
    /// content.
    pub includes: Vec<String>,
    /// Late cross-file type includes: types referenced only by pointer,
    /// included after the content for their codec definitions.
    pub late_includes: Vec<String>,
    /// Rendered code blocks.
    pub blocks: Vec<String>,
}
