//! Target language enumeration and helpers.

use mudu::common::result::RS;
use mudu_binding::universal::uni_scalar::UniScalar;

use crate::lang_impl;
use crate::lang_impl::lang::lang_data_type::uni_data_type_to_name;
use mudu_binding::universal::uni_dat_type::UniDatType;

/// Supported target languages.
#[derive(Debug, PartialOrd, PartialEq, Eq, Copy, Clone)]
pub enum LangKind {
    /// Rust.
    Rust,
    /// C#.
    CSharp,
}

impl LangKind {
    /// Return the canonical language name.
    pub fn to_str(&self) -> &'static str {
        match self {
            LangKind::Rust => "rust",
            LangKind::CSharp => "csharp",
        }
    }

    /// Parse a language name into a [`LangKind`].
    pub fn from_name(lang: &str) -> Option<LangKind> {
        let s = lang.to_lowercase();
        match s.as_str() {
            "rust" => Some(LangKind::Rust),
            "csharp" => Some(LangKind::CSharp),
            _ => None,
        }
    }

    /// Return the language-specific name of a scalar type.
    pub fn name_of_scalar(&self, p: &UniScalar) -> RS<String> {
        Ok(lang_impl::lang_scalar_name(self, p))
    }

    /// Return the language-specific name of a WIT data type.
    pub fn name_of_wit_type(&self, wit_type: &UniDatType) -> RS<String> {
        uni_data_type_to_name(wit_type, self)
    }

    /// Return the file extension for the language.
    pub fn extension(&self) -> &'static str {
        match self {
            LangKind::Rust => "rs",
            LangKind::CSharp => "cs",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LangKind;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_dat_type::UniDatType;
    use mudu_binding::universal::uni_scalar::UniScalar;

    #[test]
    fn to_str_and_extension() {
        assert_eq!(LangKind::Rust.to_str(), "rust");
        assert_eq!(LangKind::CSharp.to_str(), "csharp");
        assert_eq!(LangKind::Rust.extension(), "rs");
        assert_eq!(LangKind::CSharp.extension(), "cs");
    }

    #[test]
    fn from_name_is_case_insensitive() {
        assert_eq!(LangKind::from_name("rust"), Some(LangKind::Rust));
        assert_eq!(LangKind::from_name("RUST"), Some(LangKind::Rust));
        assert_eq!(LangKind::from_name("csharp"), Some(LangKind::CSharp));
        assert_eq!(LangKind::from_name("CSharp"), Some(LangKind::CSharp));
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
            LangKind::Rust.name_of_wit_type(&UniDatType::Scalar(UniScalar::Bool))?,
            "bool"
        );
        assert_eq!(
            LangKind::CSharp.name_of_wit_type(&UniDatType::Array(Box::new(UniDatType::Scalar(
                UniScalar::I32
            ))))?,
            "List<int>"
        );
        Ok(())
    }
}
