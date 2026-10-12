use super::pad;

/// One branch of a lenient integer-keyed map decode (a record field or a
/// request parameter).
pub struct CMapEntry {
    /// 1-based field/parameter number (the wire map key).
    pub number: u32,
    /// Decode statements for the value, indented for the branch body
    /// (`indent + 16` spaces, joined).
    pub decode_stmts: String,
}

/// Render the lenient map-decode loop shared by record and request decoders:
/// integer keys of any width, unknown keys skipped, missing keys defaulted
/// (the caller `memset`s the target beforehand).
pub fn c_map_decode_loop(entries: &[CMapEntry], indent: usize) -> String {
    let pad = pad(indent);
    let mut lines = Vec::new();
    lines.push(format!("{}{{", pad));
    lines.push(format!("{}    uint32_t count = mpr_map_header(r);", pad));
    lines.push(format!(
        "{}    for (uint32_t i = 0; i < count && mpr_ok(r); i++) {{",
        pad
    ));
    lines.push(format!("{}        if (mpr_next_is_integer(r)) {{", pad));
    lines.push(format!("{}            uint64_t key = mpr_u64(r);", pad));
    if entries.is_empty() {
        lines.push(format!("{}            (void)mpr_u64(r);", pad));
        lines.push(format!("{}            mpr_skip(r);", pad));
    } else {
        for (index, entry) in entries.iter().enumerate() {
            if index == 0 {
                lines.push(format!(
                    "{}            if (key == {}u) {{",
                    pad, entry.number
                ));
            } else {
                lines.push(format!(
                    "{}            }} else if (key == {}u) {{",
                    pad, entry.number
                ));
            }
            lines.push(entry.decode_stmts.clone());
        }
        lines.push(format!("{}            }} else {{", pad));
        lines.push(format!("{}                mpr_skip(r);", pad));
        lines.push(format!("{}            }}", pad));
    }
    lines.push(format!("{}        }} else {{", pad));
    lines.push(format!("{}            mpr_skip(r);", pad));
    lines.push(format!("{}            mpr_skip(r);", pad));
    lines.push(format!("{}        }}", pad));
    lines.push(format!("{}    }}", pad));
    lines.push(format!("{}}}", pad));
    lines.join("\n")
}
