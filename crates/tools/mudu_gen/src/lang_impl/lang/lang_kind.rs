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
    /// Python.
    Python,
    /// C (freestanding guest).
    C,
    /// Go (TinyGo guest).
    Go,
}

impl LangKind {
    /// Return the canonical language name.
    pub fn to_str(&self) -> &'static str {
        match self {
            LangKind::Rust => "rust",
            LangKind::CSharp => "csharp",
            LangKind::AssemblyScript => "assemblyscript",
            LangKind::Python => "python",
            LangKind::C => "c",
            LangKind::Go => "go",
        }
    }

    /// Parse a language name into a [`LangKind`].
    pub fn from_name(lang: &str) -> Option<LangKind> {
        let s = lang.to_lowercase();
        match s.as_str() {
            "rust" => Some(LangKind::Rust),
            "csharp" => Some(LangKind::CSharp),
            "assemblyscript" => Some(LangKind::AssemblyScript),
            "python" => Some(LangKind::Python),
            "c" => Some(LangKind::C),
            "go" => Some(LangKind::Go),
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
    ///
    /// C entities are generated as a single self-contained header per table
    /// (the row struct plus `static inline` codecs/helpers), so the C
    /// extension is `h`, not `c`: the mpm-crate pipeline then needs no
    /// source-list change, only an extra include path.
    pub fn extension(&self) -> &'static str {
        match self {
            LangKind::Rust => "rs",
            LangKind::CSharp => "cs",
            LangKind::AssemblyScript => "ts",
            LangKind::Python => "py",
            LangKind::C => "h",
            LangKind::Go => "go",
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
        assert_eq!(LangKind::Python.to_str(), "python");
        assert_eq!(LangKind::C.to_str(), "c");
        assert_eq!(LangKind::Go.to_str(), "go");
        assert_eq!(LangKind::Rust.extension(), "rs");
        assert_eq!(LangKind::CSharp.extension(), "cs");
        assert_eq!(LangKind::AssemblyScript.extension(), "ts");
        assert_eq!(LangKind::Python.extension(), "py");
        // C entities are single self-contained headers; see extension().
        assert_eq!(LangKind::C.extension(), "h");
        assert_eq!(LangKind::Go.extension(), "go");
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
        assert_eq!(LangKind::from_name("python"), Some(LangKind::Python));
        assert_eq!(LangKind::from_name("Python"), Some(LangKind::Python));
        assert_eq!(LangKind::from_name("c"), Some(LangKind::C));
        assert_eq!(LangKind::from_name("C"), Some(LangKind::C));
        assert_eq!(LangKind::from_name("go"), Some(LangKind::Go));
        assert_eq!(LangKind::from_name("GO"), Some(LangKind::Go));
        assert_eq!(LangKind::from_name("java"), None);
    }

    #[test]
    fn name_of_scalar() -> RS<()> {
        assert_eq!(LangKind::Rust.name_of_scalar(&UniScalar::I32)?, "i32");
        assert_eq!(
            LangKind::CSharp.name_of_scalar(&UniScalar::String)?,
            "string"
        );
        assert_eq!(LangKind::C.name_of_scalar(&UniScalar::I64)?, "int64_t");
        assert_eq!(LangKind::Go.name_of_scalar(&UniScalar::Bool)?, "bool");
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
        assert_eq!(
            LangKind::Python.name_of_wit_type(&UniDataType::Scalar(UniScalar::I32))?,
            "int"
        );
        assert_eq!(
            LangKind::C.name_of_wit_type(&UniDataType::Scalar(UniScalar::I64))?,
            "int64_t"
        );
        assert_eq!(
            LangKind::Go.name_of_wit_type(&UniDataType::Scalar(UniScalar::F64))?,
            "float64"
        );
        Ok(())
    }
}
