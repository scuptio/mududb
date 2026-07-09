//! Target language enumeration and helpers.

use mudu::common::result::RS;
use mudu_binding::universal::uni_scalar::UniScalar;

use crate::lang_impl;
use crate::lang_impl::lang::lang_data_type::uni_data_type_to_name;
use mudu_binding::universal::uni_data_type::UniDataType;

/// Supported target languages.
#[derive(Debug, PartialOrd, PartialEq, Eq, Copy, Clone)]
pub enum LangKind {
    /// Rust.
    Rust,
    /// C#.
    CSharp,
    /// AssemblyScript.
    AssemblyScript,
}

impl LangKind {
    /// Return the canonical language name.
    pub fn to_str(&self) -> &'static str {
        match self {
            LangKind::Rust => "rust",
            LangKind::CSharp => "csharp",
            LangKind::AssemblyScript => "assemblyscript",
        }
    }

    /// Parse a language name into a [`LangKind`].
    pub fn from_name(lang: &str) -> Option<LangKind> {
        let s = lang.to_lowercase();
        match s.as_str() {
            "rust" => Some(LangKind::Rust),
            "csharp" => Some(LangKind::CSharp),
            "assemblyscript" => Some(LangKind::AssemblyScript),
            _ => None,
        }
    }

    /// Return the language-specific name of a scalar type.
    pub fn name_of_scalar(&self, p: &UniScalar) -> RS<String> {
        Ok(lang_impl::lang_scalar_name(self, p))
    }

    /// Return the language-specific name of a WIT data type.
    pub fn name_of_wit_type(&self, wit_type: &UniDataType) -> RS<String> {
        uni_data_type_to_name(wit_type, self)
    }

    /// Return the file extension for the language.
    pub fn extension(&self) -> &'static str {
        match self {
            LangKind::Rust => "rs",
            LangKind::CSharp => "cs",
            LangKind::AssemblyScript => "ts",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LangKind;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_scalar::UniScalar;

    #[test]
    fn to_str_and_extension() {
        assert_eq!(LangKind::Rust.to_str(), "rust");
        assert_eq!(LangKind::CSharp.to_str(), "csharp");
        assert_eq!(LangKind::AssemblyScript.to_str(), "assemblyscript");
        assert_eq!(LangKind::Rust.extension(), "rs");
        assert_eq!(LangKind::CSharp.extension(), "cs");
        assert_eq!(LangKind::AssemblyScript.extension(), "ts");
    }

    #[test]
    fn from_name_is_case_insensitive() {
        assert_eq!(LangKind::from_name("rust"), Some(LangKind::Rust));
        assert_eq!(LangKind::from_name("RUST"), Some(LangKind::Rust));
        assert_eq!(LangKind::from_name("csharp"), Some(LangKind::CSharp));
        assert_eq!(LangKind::from_name("CSharp"), Some(LangKind::CSharp));
        assert_eq!(
            LangKind::from_name("AssemblyScript"),
            Some(LangKind::AssemblyScript)
        );
        assert_eq!(LangKind::from_name("java"), None);
    }

    #[test]
    fn name_of_scalar() -> RS<()> {
        assert_eq!(LangKind::Rust.name_of_scalar(&UniScalar::I32)?, "i32");
        assert_eq!(
            LangKind::CSharp.name_of_scalar(&UniScalar::String)?,
            "string"
        );
        Ok(())
    }

    #[test]
    fn name_of_wit_type() -> RS<()> {
        assert_eq!(
            LangKind::Rust.name_of_wit_type(&UniDataType::Scalar(UniScalar::Bool))?,
            "bool"
        );
        assert_eq!(
            LangKind::CSharp.name_of_wit_type(&UniDataType::Array(Box::new(
                UniDataType::Scalar(UniScalar::I32)
            )))?,
            "List<int>"
        );
        assert_eq!(
            LangKind::AssemblyScript.name_of_wit_type(&UniDataType::Scalar(UniScalar::I32))?,
            "i32"
        );
        Ok(())
    }
}
