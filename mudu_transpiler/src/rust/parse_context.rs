//! Parse context for the Rust front-end.
//!
//! `ParseContext` tracks the original source text, discovered Mudu procedures,
//! call dependencies, and source positions that need `async`/`await` rewriting.

use crate::rust::function::Function;
use crate::rust::template_proc::{ArgumentInfo, ProcedureInfo, ReturnInfo, TemplateProc};
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use mudu::utils::case_convert::{to_kebab_case, to_pascal_case};
use mudu_binding::universal::uni_type_desc::UniTypeDesc;
use mudu_contract::procedure::proc;
use mudu_contract::procedure::proc_desc::ProcDesc;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use tree_sitter::Node;

/// Source position replacement recorded for a `use` path refactor.
#[derive(Debug, Clone)]
pub struct UseRefactor {
    /// Start position of the text to replace.
    pub start_position: Position,
    /// End position of the text to replace.
    pub end_position: Position,
    /// Original source text.
    pub src_string: String,
    /// Replacement source text.
    pub dst_string: String,
}

/// Mutable state accumulated while parsing and rewriting a Rust source file.
#[derive(Debug)]
pub struct ParseContext {
    /// Original source text.
    pub text: String,
    /// Names of Mudu system calls that require async handling.
    pub sys_call: HashSet<String>,
    /// Callee key -> caller value.
    pub call_dependencies: HashMap<String, HashSet<String>>,
    /// End positions of calls per function, with a flag for system calls.
    pub position_call_end: HashMap<String, Vec<(Position, bool)>>,
    /// Start positions of function declarations and whether they are async.
    pub position_fn_start: HashMap<String, (Position, bool)>,
    /// Discovered Mudu procedures keyed by function name.
    pub mudu_procedure: HashMap<String, Function>,
    /// Pending `use` path replacements.
    pub position_refactor_use: Vec<UseRefactor>,
    /// Source lines.
    pub lines: Vec<String>,
    /// Optional `(src, dst)` module path pair for `use` rewriting.
    pub refactor_src_dst_mod: Option<(Vec<String>, Vec<String>)>,
}

impl ParseContext {
    /// Create a new context from source text and optional module rewrite paths.
    pub fn new(text: String, src_mod: Option<String>, dst_mod: Option<String>) -> Self {
        let mut sys_call = HashSet::new();
        sys_call.insert("mudu_query".to_string());
        sys_call.insert("mudu_command".to_string());
        sys_call.insert("mudu_open".to_string());
        sys_call.insert("mudu_close".to_string());
        sys_call.insert("mudu_get".to_string());
        sys_call.insert("mudu_put".to_string());
        sys_call.insert("mudu_range".to_string());
        let lines: Vec<String> = text.lines().map(|s| s.to_string()).collect::<Vec<_>>();
        let refactor_src_dst_mod = if let Some(src) = src_mod
            && let Some(dst) = dst_mod
        {
            let src_path = mod_path_to_vec(&src);
            let dst_path = mod_path_to_vec(&dst);
            if src_path == dst_path || src_path.len() != dst_path.len() {
                None
            } else {
                Some((src_path, dst_path))
            }
        } else {
            None
        };
        Self {
            text,
            sys_call,
            call_dependencies: Default::default(),
            position_call_end: Default::default(),
            position_fn_start: Default::default(),
            mudu_procedure: Default::default(),
            position_refactor_use: Default::default(),
            lines,
            refactor_src_dst_mod,
        }
    }

    /// Extract the source text covered by `node`.
    pub fn node_text(&self, node: &Node) -> RS<String> {
        let s = node
            .utf8_text(self.text.as_bytes())
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode utf8 error", e))?;
        Ok(s.to_string())
    }

    /// Return whether `name` is a Mudu system call.
    pub fn is_sys_call(&self, name: &str) -> bool {
        self.sys_call.contains(name)
    }

    /// Record the end position of a call inside `fn_name`.
    pub fn add_func_call_end_position(
        &mut self,
        fn_name: String,
        end_position: Position,
        sys_call: bool,
    ) {
        let opt = self.position_call_end.get_mut(&fn_name);
        if let Some(vec) = opt {
            vec.push((end_position, sys_call));
        } else {
            self.position_call_end
                .insert(fn_name, vec![(end_position, sys_call)]);
        }
    }

    /// Record that `caller` calls `callee`.
    pub fn add_call_dependency(&mut self, caller: &str, callee: &str) {
        if let Some(set) = self.call_dependencies.get_mut(callee) {
            set.insert(caller.to_owned());
        } else {
            let caller_set = HashSet::from_iter(vec![caller.to_owned()]);
            self.call_dependencies.insert(callee.to_owned(), caller_set);
        }
    }

    /// Render the source text with `async`/`await` annotations applied.
    pub fn render_async(&self) -> RS<String> {
        let positions = self.update_positions();
        let mut lines = self.lines.clone();
        for (up_ty, pos) in positions {
            let line = &mut lines[pos.row];
            let (range, to_replaced_str, replace_str) = up_ty.to_replace_string()?;
            if line.len() < range.start
                || line.len() < range.end
                || range.start > range.end
                || line[range.start..range.end] != to_replaced_str
            {
                return Err(mudu_error!(
                    ErrorCode::Internal,
                    "render error, range mismatch, possible a bug"
                ));
            }
            line.replace_range(range.start..range.end, &replace_str)
        }
        Ok(lines.join("\n"))
    }

    /// Generate procedure descriptors for all discovered procedures.
    pub fn gen_procedure_desc_list(
        &self,
        module_name: &str,
        custom_types: &UniTypeDesc,
    ) -> RS<Vec<ProcDesc>> {
        let mut vec = Vec::new();
        for function in self.mudu_procedure.values() {
            let proc_desc = function.to_proc_desc(module_name, custom_types)?;
            vec.push(proc_desc);
        }
        Ok(vec)
    }

    /// Render the final source file, including generated procedure wrappers.
    pub fn render_source(&self, module_name: String, enable_async: bool) -> RS<String> {
        let mut src = if enable_async {
            self.render_async()?
        } else {
            self.text.clone()
        };
        src = if enable_async {
            src.replace("sys_interface::sync_api", "sys_interface::async_api")
                .replace("sys_interface::api", "sys_interface::async_api")
        } else {
            src.replace("sys_interface::api", "sys_interface::sync_api")
        };
        for function in self.mudu_procedure.values() {
            let template = function_to_template(function, module_name.clone(), enable_async)?;
            let source = template
                .render()
                .map_err(|e| mudu_error!(ErrorCode::Encode, "encode mudu error", e))?;
            src.push_str(&source);
        }

        Ok(src)
    }

    /// Rewrite internal state so that Mudu system calls and their transitive
    /// callers are marked async.
    pub fn tran_to_async(&mut self) {
        let mut position_fn_start = HashMap::new();
        let mut position_call_end = HashMap::new();
        std::mem::swap(&mut position_fn_start, &mut self.position_fn_start);
        std::mem::swap(&mut position_call_end, &mut self.position_call_end);
        let mut async_callee = self.sys_call.clone();
        let mut async_caller = HashSet::default();
        while !(async_caller.is_empty() && async_callee.is_empty()) {
            self.update_async_await_walk_dependency(
                &mut async_caller,
                &mut async_callee,
                &mut position_fn_start,
                &mut position_call_end,
            );
        }
        self.position_fn_start = position_fn_start;
        self.position_call_end = position_call_end;
        for (name, function) in self.mudu_procedure.iter_mut() {
            let opt = self.position_fn_start.get(name);
            if let Some((_, is_async)) = opt
                && *is_async
            {
                function.is_async = true;
            }
        }
    }

    fn get_caller_of_callee(&self, callee: &String) -> Option<&HashSet<String>> {
        self.call_dependencies.get(callee)
    }

    fn update_async_await_walk_dependency(
        &self,
        callers: &mut HashSet<String>,
        callees: &mut HashSet<String>,
        position_fn_start: &mut HashMap<String, (Position, bool)>,
        position_call_end: &mut HashMap<String, Vec<(Position, bool)>>,
    ) {
        for callee in callees.iter() {
            self.mark_all_async_caller(callee, callers, position_fn_start);
        }
        callees.clear();
        for caller in callers.iter() {
            self.mark_all_async_callee(caller, callees, position_call_end);
        }
        callers.clear();
    }

    fn mark_all_async_caller(
        &self,
        callee: &String,
        callers: &mut HashSet<String>,
        position_fn_start: &mut HashMap<String, (Position, bool)>,
    ) {
        let _set = HashSet::default();
        let set = self.get_caller_of_callee(callee).unwrap_or(&_set);
        for caller in set {
            let opt = position_fn_start.get_mut(caller);
            if let Some((_pos, is_async)) = opt
                && !*is_async
            {
                *is_async = true;
                callers.insert(caller.clone());
            }
            self.mark_all_async_caller(caller, callers, position_fn_start);
        }
    }

    fn mark_all_async_callee(
        &self,
        caller: &String,
        callees: &mut HashSet<String>,
        position_call_end: &mut HashMap<String, Vec<(Position, bool)>>,
    ) {
        let mut _vec = Vec::new();
        let vec = position_call_end.get_mut(caller).unwrap_or(&mut _vec);
        for (_, is_async) in vec {
            if !*is_async {
                *is_async = true;
                callees.insert(caller.clone());
            }
        }
    }

    fn update_positions(&self) -> Vec<(UpdateType, Position)> {
        let mut vec = Vec::new();
        for (pos, is_async) in self.position_fn_start.values() {
            if *is_async {
                vec.push((UpdateType::Async(*pos), *pos));
            }
        }
        for vec_call_pos in self.position_call_end.values() {
            for (pos, is_async) in vec_call_pos.iter() {
                if *is_async {
                    vec.push((UpdateType::Await(*pos), *pos));
                }
            }
        }
        for use_refactor in self.position_refactor_use.iter() {
            vec.push((
                UpdateType::Use(use_refactor.clone()),
                use_refactor.start_position,
            ))
        }

        // from bottom to up,
        // and right to left <--
        vec.sort_by(|a, b| {
            let ord = a.1.row.cmp(&b.1.row).reverse();
            if ord.is_eq() {
                a.1.col.cmp(&b.1.col).reverse()
            } else {
                ord
            }
        });
        vec
    }
}

fn mod_path_to_vec(str: &str) -> Vec<String> {
    str.split("::").map(|x| x.to_string()).collect()
}

/// Zero-based row/column position in a source file.
#[derive(Debug, Clone, Copy)]
pub struct Position {
    /// Row index.
    pub row: usize,
    /// Column index.
    pub col: usize,
}

impl Position {
    /// Convert a `tree_sitter::Point` into a `Position`.
    pub fn from_ts(pos: tree_sitter::Point) -> Self {
        Self {
            row: pos.row,
            col: pos.column,
        }
    }
}

enum UpdateType {
    Async(Position),
    Await(Position),
    Use(UseRefactor),
}

impl UpdateType {
    fn to_replace_string(&self) -> RS<(Range<usize>, String, String)> {
        match self {
            UpdateType::Async(position) => Ok((
                Range {
                    start: position.col,
                    end: position.col,
                },
                "".to_string(),
                "async ".to_string(),
            )),
            UpdateType::Await(position) => Ok((
                Range {
                    start: position.col,
                    end: position.col,
                },
                "".to_string(),
                ".await".to_string(),
            )),
            UpdateType::Use(use_refactor) => {
                if use_refactor.start_position.row != use_refactor.end_position.row {
                    return Err(mudu_error!(
                        ErrorCode::Internal,
                        "start position and end position must be in the same row"
                    ));
                }
                Ok((
                    Range {
                        start: use_refactor.start_position.col,
                        end: use_refactor.end_position.col,
                    },
                    use_refactor.src_string.clone(),
                    use_refactor.dst_string.clone(),
                ))
            }
        }
    }
}

fn function_to_template(
    function: &Function,
    module_name: String,
    enable_async: bool,
) -> RS<TemplateProc> {
    let fn_name = function.name.clone();
    let mod_name = format!("{}{}", proc::MUDU_PROC_PREFIX_MOD, fn_name);
    let fn_exported_name = format!("{}{}", proc::MUDU_PROC_P2_PREFIX, fn_name);
    let wit_fn_exported_name = to_kebab_case(&fn_exported_name);
    let fn_inner_name = format!("{}{}", proc::MUDU_PROC_INNER_PREFIX_P2, fn_name);
    let guest_struct_name = format!(
        "{}{}",
        proc::MUDU_PROC_PREFIX_GUEST,
        to_pascal_case(&fn_name)
    );
    let fn_argv_desc = format!("{}{}", proc::MUDU_PROC_ARGV_DESC_PREFIX, fn_name);
    let fn_result_desc = format!("{}{}", proc::MUDU_PROC_RESULT_DESC_PREFIX, fn_name);
    let fn_proc_desc = format!("{}{}", proc::MUDU_PROC_PROC_DESC_PREFIX, fn_name);
    let mut wit_async_true = String::new();
    let mut opt_async = String::new();
    let mut opt_dot_await = String::new();
    let mut opt_underline_async = String::new();
    if enable_async && function.is_async {
        wit_async_true = "async: true".to_string();
        opt_async = "async".to_string();
        opt_dot_await = ".await".to_string();
        opt_underline_async = "_async".to_string();
    }
    let return_tuple = function.return_type.as_ref().map_or_else(
        || Ok::<_, MuduError>(Vec::new()),
        |return_type| {
            let ret_types = return_type.as_ret_type()?;
            Ok(return_type
                .to_ret_type_str()?
                .into_iter()
                .enumerate()
                .map(|(i, e)| ReturnInfo {
                    ret_type: e,
                    is_binary: ret_types[i].is_vec_u8(),
                })
                .collect())
        },
    )?;
    //ignore the first argument, ID
    let argument_list = function.arg_list[1..]
        .iter()
        .enumerate()
        .map(|(i, (n, t))| ArgumentInfo {
            arg_name: n.clone(),
            arg_type: t.to_type_str(),
            arg_index: i,
            is_binary: t.is_vec_u8(),
        })
        .collect::<Vec<ArgumentInfo>>();

    Ok(TemplateProc {
        procedure: ProcedureInfo {
            mod_name,
            fn_name,
            wit_fn_exported_name,
            wit_async_true,
            fn_exported_name,
            fn_inner_name,
            guest_struct_name,
            fn_argv_desc,
            fn_result_desc,
            fn_proc_desc,
            package_name: module_name,
            argument_list,
            return_tuple,
            return_len: function
                .return_type
                .as_ref()
                .map_or(Ok::<_, MuduError>(0), |return_type| {
                    Ok(return_type.as_ret_type()?.len())
                })?,
            opt_async,
            opt_dot_await,
            opt_underline_async,
        },
    })
}
