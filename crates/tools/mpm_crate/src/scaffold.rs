//! Project scaffolding: name validation, template rendering and file
//! materialization.

use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::ValueEnum;
use mudu_sys::fs::sync::{sync_create_dir_all, sync_path_exists, sync_read_dir, sync_write};

use crate::templates::templates_for;

/// Longest accepted project name.
const MAX_NAME_LEN: usize = 64;

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

/// Arguments for [`scaffold`].
#[derive(Clone, Debug)]
pub struct ScaffoldArgs {
    /// Project name (also the directory name).
    pub name: String,
    /// Template language.
    pub lang: Lang,
    /// Parent directory in which the project directory is created; defaults
    /// to the current directory.
    pub path: Option<PathBuf>,
    /// Path to a mududb repository checkout; rewrites SDK dependencies to
    /// path dependencies instead of published-package placeholders.
    pub sdk_path: Option<PathBuf>,
}

/// Names derived from the validated project name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectNames {
    /// The project name as given (kebab-case).
    pub project_name: String,
    /// snake_case form: wasm file stem, `package.cfg.json` name, Cargo
    /// package name and C# `AssemblyName`.
    pub module_name: String,
    /// kebab-case form: npm package name and WIT world name.
    pub kebab_name: String,
    /// PascalCase form: C# project file name and namespaces.
    pub pascal_name: String,
}

/// What [`scaffold`] produced.
#[derive(Clone, Debug)]
pub struct ScaffoldReport {
    /// Directory the project was written to.
    pub project_dir: PathBuf,
    /// Template language that was rendered.
    pub lang: Lang,
    /// Names derived from the project name.
    pub names: ProjectNames,
    /// Files written, relative to `project_dir`.
    pub files: Vec<PathBuf>,
}

/// Validates the project name: starts with an ASCII letter, then only
/// lowercase ASCII letters, digits and `-`; at most 64 characters.
pub fn validate_project_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("project name must not be empty");
    }
    if name.len() > MAX_NAME_LEN {
        bail!("project name '{name}' is longer than {MAX_NAME_LEN} characters");
    }
    let mut chars = name.chars();
    if let Some(first) = chars.next()
        && !first.is_ascii_lowercase()
    {
        bail!("project name '{name}' must start with a lowercase letter");
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'))
    {
        bail!(
            "project name '{name}' contains invalid character '{bad}'; \
             use only lowercase letters, digits and '-'"
        );
    }
    Ok(())
}

/// Derives the snake_case / kebab-case / PascalCase forms of a validated
/// project name.
pub fn derive_names(name: &str) -> Result<ProjectNames> {
    validate_project_name(name)?;
    let module_name = name.replace('-', "_");
    let pascal_name = name
        .split('-')
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<String>();
    Ok(ProjectNames {
        project_name: name.to_string(),
        module_name,
        kebab_name: name.to_string(),
        pascal_name,
    })
}

/// Returns the SDK subdirectory (relative to the repository root) that a
/// `--sdk-path` checkout must contain for `lang`, or `None` when the
/// language template has no SDK dependency to rewrite.
fn sdk_subdir(lang: Lang) -> Option<&'static str> {
    match lang {
        Lang::Rust => Some("crates/sdk/mududb"),
        Lang::AssemblyScript => Some("crates/sdk/bindings/assemblyscript"),
        Lang::Csharp | Lang::C | Lang::Go => None,
        Lang::Python => Some("crates/sdk/bindings/python"),
    }
}

/// Renders `text`, replacing every `{{key}}` occurrence with its value.
fn render(text: &str, pairs: &[(&str, String)]) -> String {
    let mut out = text.to_string();
    for (key, value) in pairs {
        out = out.replace(&format!("{{{{{key}}}}}"), value);
    }
    out
}

/// Converts a path to a forward-slash string so it can be embedded into
/// TOML/JSON template files on every platform.
fn slash_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Makes `path` absolute by resolving it against the current directory when
/// it is relative.
fn absolutize(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(mudu_sys::env_var::current_dir()?.join(path))
    }
}

/// Builds the placeholder values for a scaffold run.
fn placeholder_pairs(
    args: &ScaffoldArgs,
    names: &ProjectNames,
) -> Result<Vec<(&'static str, String)>> {
    let mut pairs = vec![
        ("project_name", names.project_name.clone()),
        ("module_name", names.module_name.clone()),
        ("kebab_name", names.kebab_name.clone()),
        ("pascal_name", names.pascal_name.clone()),
    ];

    match (args.lang, &args.sdk_path) {
        (Lang::Csharp | Lang::C | Lang::Go, Some(_)) => {
            bail!(
                "--sdk-path is not used for {} projects: the template is self-contained",
                args.lang
            );
        }
        (_, Some(sdk_path)) => {
            let sdk_path = absolutize(sdk_path)?;
            let subdir = sdk_subdir(args.lang)
                .ok_or_else(|| anyhow!("no SDK dependency for language {}", args.lang))?;
            let dep_path = sdk_path.join(subdir);
            if !sync_path_exists(&dep_path) {
                bail!(
                    "--sdk-path '{}' does not look like a mududb repository: '{}' not found",
                    sdk_path.display(),
                    dep_path.display()
                );
            }
            let dep = slash_string(&dep_path);
            match args.lang {
                Lang::Rust => pairs.push((
                    "rust_sdk_dep",
                    format!("mududb = {{ path = \"{dep}\", features = [\"async\"] }}"),
                )),
                Lang::AssemblyScript => {
                    pairs.push(("as_sdk_dep", format!("\"@mududb/mududb\": \"file:{dep}\"")))
                }
                Lang::Python => pairs.push(("py_sdk_path", dep)),
                Lang::Csharp | Lang::C | Lang::Go => {}
            }
        }
        (_, None) => match args.lang {
            Lang::Rust => pairs.push((
                "rust_sdk_dep",
                "mududb = { version = \"0.1\", features = [\"async\"] }".to_string(),
            )),
            Lang::AssemblyScript => {
                pairs.push(("as_sdk_dep", "\"@mududb/mududb\": \"^0.1.0\"".to_string()))
            }
            Lang::Python => pairs.push(("py_sdk_path", "CHANGE_ME_SITE_PACKAGES".to_string())),
            Lang::Csharp | Lang::C | Lang::Go => {}
        },
    }
    Ok(pairs)
}

/// Creates a new MuduDB `.mpk` application project from the embedded
/// templates.
pub fn scaffold(args: &ScaffoldArgs) -> Result<ScaffoldReport> {
    let names = derive_names(&args.name)?;
    let pairs = placeholder_pairs(args, &names)?;

    let parent = args.path.clone().unwrap_or_else(|| PathBuf::from("."));
    let project_dir = parent.join(&args.name);
    if sync_path_exists(&project_dir)
        && !sync_read_dir(&project_dir)
            .with_context(|| format!("cannot read directory '{}'", project_dir.display()))?
            .is_empty()
    {
        bail!(
            "destination '{}' already exists and is not empty",
            project_dir.display()
        );
    }
    sync_create_dir_all(&project_dir)
        .with_context(|| format!("cannot create directory '{}'", project_dir.display()))?;

    let mut files = Vec::new();
    for template in templates_for(args.lang) {
        let relative = template
            .relative_path
            .strip_suffix(".tmpl")
            .ok_or_else(|| {
                anyhow!(
                    "template '{}' does not end with .tmpl",
                    template.relative_path
                )
            })?;
        let relative = render(relative, &pairs);
        let content = render(template.content, &pairs);
        if content.contains("{{") {
            return Err(anyhow!(
                "template '{}' still contains an unresolved placeholder after rendering",
                template.relative_path
            ));
        }
        let target = project_dir.join(&relative);
        if let Some(parent) = target.parent() {
            sync_create_dir_all(parent)
                .with_context(|| format!("cannot create directory '{}'", parent.display()))?;
        }
        sync_write(&target, content.as_bytes())
            .with_context(|| format!("cannot write file '{}'", target.display()))?;
        files.push(PathBuf::from(relative));
    }

    Ok(ScaffoldReport {
        project_dir,
        lang: args.lang,
        names,
        files,
    })
}
