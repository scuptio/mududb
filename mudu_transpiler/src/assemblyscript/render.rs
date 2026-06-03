use crate::assemblyscript::desc::gen_procedure_shim_inputs;
use crate::assemblyscript::procedure::{AsParam, AsProcedure};
use crate::procedure_shim::{
    ProcedureShimConfig, procedure_adapter_name, procedure_interface_name, render_rust_p2_wrapper,
};
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::collections::HashSet;
use std::path::{Component, Path};

#[derive(Template)]
#[template(path = "assemblyscript/adapter.ts.jinja", escape = "none")]
struct AdapterTemplate<'a> {
    source_import_path: String,
    procedures: &'a [AdapterProcedure],
}

pub(super) fn render_adapter_source(
    input_path: &Path,
    output_path: &Path,
    procedures: &[AsProcedure],
) -> RS<String> {
    let mut names = NameRegistry::default();
    let adapter_procedures = procedures
        .iter()
        .map(|procedure| AdapterProcedure::from_procedure(procedure, &mut names))
        .collect::<Vec<_>>();
    AdapterTemplate {
        source_import_path: source_import_path(input_path, output_path),
        procedures: &adapter_procedures,
    }
    .render()
    .map_err(|e| m_error!(EC::EncodeErr, "render assemblyscript adapter error", e))
}

struct AdapterProcedure {
    name: String,
    source_alias: String,
    adapter_name: String,
    result_expr: String,
    return_value_ctor: String,
    returns_result: bool,
    args: Vec<AdapterArg>,
    call_args: Vec<String>,
}

struct AdapterArg {
    name: String,
    value_index: usize,
    value_getter: String,
}

impl AdapterProcedure {
    fn from_procedure(procedure: &AsProcedure, names: &mut NameRegistry) -> Self {
        let value_args = procedure
            .params
            .iter()
            .skip(1)
            .enumerate()
            .map(|(index, param)| AdapterArg::from_param(index, param))
            .collect::<Vec<_>>();
        let mut call_args = Vec::with_capacity(procedure.params.len());
        call_args.push("id".to_string());
        call_args.extend(value_args.iter().map(|arg| arg.name.clone()));
        let result_expr = if procedure.returns_result {
            "result.unwrap()".to_string()
        } else {
            "result".to_string()
        };

        Self {
            name: procedure.name.clone(),
            source_alias: names.claim("__mudu_proc_", &procedure.name),
            adapter_name: procedure.adapter_name(),
            result_expr,
            return_value_ctor: procedure.return_value_type.value_ctor().to_string(),
            returns_result: procedure.returns_result,
            args: value_args,
            call_args,
        }
    }
}

impl AdapterArg {
    fn from_param(value_index: usize, param: &AsParam) -> Self {
        Self {
            name: param.name.clone(),
            value_index,
            value_getter: param.value_type.value_getter().to_string(),
        }
    }
}

pub(super) fn render_wit(procedures: &[AsProcedure]) -> String {
    let mut out = String::from("package mududb:component-shim;\n\n");
    for procedure in procedures {
        let interface_name = procedure_interface_name(&procedure.name);
        let adapter_name = procedure_adapter_name(&procedure.name);
        out.push_str(&format!(
            "interface {interface_name} {{\n    use types.{{error, oid}};\n    use system.{{value-list}};\n\n    {adapter_name}: func(id: oid, values: borrow<value-list>) -> result<value-list, error>;\n}}\n\n"
        ));
    }
    out.push_str("world procedure-api {\n    import types;\n    import system;\n");
    for procedure in procedures {
        out.push_str(&format!(
            "    export {};\n",
            procedure_interface_name(&procedure.name)
        ));
    }
    out.push_str("}\n");
    out
}

#[derive(Default)]
struct NameRegistry {
    names: HashSet<String>,
}

impl NameRegistry {
    fn claim(&mut self, prefix: &str, input: &str) -> String {
        let mut base = String::with_capacity(prefix.len() + input.len());
        base.push_str(prefix);
        base.push_str(&sanitize_identifier(input));

        let mut candidate = base.clone();
        let mut suffix = 2;
        while !self.names.insert(candidate.clone()) {
            candidate = format!("{base}_{suffix}");
            suffix += 1;
        }
        candidate
    }
}

fn sanitize_identifier(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for (index, ch) in input.chars().enumerate() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            if index == 0 && ch.is_ascii_digit() {
                out.push('_');
            }
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() { "_".to_string() } else { out }
}

fn source_import_path(input_path: &Path, output_path: &Path) -> String {
    let output_dir = output_path.parent().unwrap_or_else(|| Path::new("."));
    let mut path = relative_path(output_dir, input_path)
        .unwrap_or_else(|| input_path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/");
    if let Some(stripped) = path.strip_suffix(".ts") {
        path = stripped.to_string();
    }
    if !path.starts_with('.') && !path.starts_with('/') {
        path = format!("./{path}");
    }
    path
}

fn relative_path(from_dir: &Path, to_file: &Path) -> Option<std::path::PathBuf> {
    let from_components = normalized_components(from_dir)?;
    let to_components = normalized_components(to_file)?;
    let common_len = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(left, right)| left == right)
        .count();

    let mut out = std::path::PathBuf::new();
    for _ in common_len..from_components.len() {
        out.push("..");
    }
    for component in &to_components[common_len..] {
        out.push(component);
    }
    Some(out)
}

fn normalized_components(path: &Path) -> Option<Vec<String>> {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) => return None,
            Component::RootDir => components.push("/".to_string()),
            Component::CurDir => {}
            Component::ParentDir => components.push("..".to_string()),
            Component::Normal(value) => components.push(value.to_string_lossy().to_string()),
        }
    }
    Some(components)
}

pub(super) fn render_rust_wrapper(procedures: &[AsProcedure], package_name: &str) -> RS<String> {
    render_rust_p2_wrapper(
        gen_procedure_shim_inputs(procedures),
        package_name,
        ProcedureShimConfig {
            generated_by: "mtp assembly-script".to_string(),
            import_module_name: "mudu_language_procedure_shim".to_string(),
            import_world_name: "language-procedure-imports".to_string(),
            helper_prefix: "mudu_lang_".to_string(),
        },
    )
}
