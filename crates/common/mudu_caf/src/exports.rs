//! Best-effort enumeration of WASM module export names: without a WIT world,
//! the manifest's `exports` fall back to the bare export names.

use crate::manifest::ExportEntry;
use wasmparser::{Parser, Payload};

/// Extracts the export names of a WASM module (best effort). Any
/// unparseable input yields an empty list.
/// `capability` is left empty (the export→capability binding is done by the
/// WIT world at load time; this crate is not involved).
pub fn extract_exports(wasm: &[u8]) -> Vec<ExportEntry> {
    let mut out = Vec::new();
    for payload in Parser::new(0).parse_all(wasm) {
        let payload = match payload {
            Ok(p) => p,
            Err(_) => return out,
        };
        if let Payload::ExportSection(reader) = payload {
            for export in reader {
                let export = match export {
                    Ok(e) => e,
                    Err(_) => return out,
                };
                out.push(ExportEntry {
                    capability: String::new(),
                    export: export.name.to_string(),
                    signature: core_kind(export.kind),
                });
            }
        }
    }
    out
}

fn core_kind(kind: wasmparser::ExternalKind) -> String {
    format!("{kind:?}").to_ascii_lowercase()
}
