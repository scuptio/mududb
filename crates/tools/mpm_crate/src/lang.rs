use clap::ValueEnum;
use std::fmt;

/// Guest language of the scaffolded project.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Lang {
    /// Rust guest compiled to a wasm32-wasip2 component.
    #[value(name = "rust", alias = "rs")]
    Rust,
    /// AssemblyScript guest compiled with asc and wasm-tools.
    #[value(name = "assemblyscript", alias = "as")]
    AssemblyScript,
    /// C# guest compiled with componentize-dotnet (.NET 10 SDK).
    #[value(name = "csharp", alias = "cs")]
    Csharp,
    /// Python guest compiled with componentize-py (experimental).
    #[value(name = "python", alias = "py")]
    Python,
    /// C/C++ guest compiled with a wasm32-targeting clang and wasm-tools.
    #[value(name = "c", alias = "cc", alias = "cpp")]
    C,
    /// Go guest compiled with TinyGo and wit-bindgen-go (experimental).
    #[value(name = "go", alias = "golang")]
    Go,
}

impl Lang {
    /// Template directory key under `templates/`.
    pub fn key(&self) -> &'static str {
        match self {
            Lang::Rust => "rust",
            Lang::AssemblyScript => "assemblyscript",
            Lang::Csharp => "csharp",
            Lang::Python => "python",
            Lang::C => "c",
            Lang::Go => "go",
        }
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}
