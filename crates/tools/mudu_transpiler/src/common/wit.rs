//! Render the byte-pipe procedure world WIT shared by the guest front-ends.

use mudu::utils::case_convert::to_kebab_case;
use mudu_contract::procedure::proc;

/// Render the byte-pipe procedure world WIT for the module: the guest imports
/// the host syscall interface `mududb:api/system` directly and exports one
/// root-level `mp2-<kebab>` byte-pipe function per procedure (the world shape
/// every non-Rust guest uses).
///
/// `extra_world_lines` carries additional world-body lines a guest runtime
/// needs (the Go/TinyGo world adds `include wasi:cli/imports@0.2.0;` for the
/// WASI 0.2 interfaces its runtime links against); each entry is emitted
/// indented on its own line ahead of the `import mududb:api/system;` line.
pub fn render_wit(proc_names: &[String], module_name: &str, extra_world_lines: &[&str]) -> String {
    let world_name = to_kebab_case(module_name);
    let mut out = format!("package mududb:{world_name};\n\n");
    out.push_str(&format!("world {world_name} {{\n"));
    for line in extra_world_lines {
        out.push_str(&format!("    {line}\n"));
    }
    out.push_str("    import mududb:api/system;\n");
    for name in proc_names {
        let export_name = to_kebab_case(&format!("{}{}", proc::MUDU_PROC_P2_PREFIX, name));
        out.push_str(&format!(
            "\n    export {export_name}: func(param: list<u8>) -> list<u8>;\n"
        ));
    }
    out.push_str("}\n");
    out
}
