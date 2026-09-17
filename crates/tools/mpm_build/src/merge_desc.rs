//! Merge multiple procedure-description files into a single description.

use anyhow::Result;
use mudu::utils::json::to_json_str;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use mudu_utils::json::read_json;
use std::path::Path;

/// Merge all `*.desc.json` files in `input_folder` into `output_desc_file`.
///
/// Files whose names end with `.desc.json` (case-insensitive) are read as
/// [`ModProcDesc`] and merged. All other files are ignored.
pub fn merge_desc_files<P: AsRef<Path>, D: AsRef<Path>>(
    input_folder: P,
    output_desc_file: D,
) -> Result<()> {
    let mut package_desc = ModProcDesc::new(Default::default());
    let dir = mudu_sys::fs::sync::sync_read_dir_entries(input_folder.as_ref())?;
    for entry in dir {
        let meta = entry.metadata()?;
        if meta.is_file() {
            let s = entry.file_name().to_string_lossy().to_string();
            if s.to_lowercase().ends_with(".desc.json") {
                let mut d = read_json::<ModProcDesc, &Path>(entry.path().as_ref())?;
                package_desc.merge(&mut d);
            }
        }
    }

    let json_str = to_json_str(&package_desc)?;
    mudu_sys::fs::sync::sync_write(output_desc_file, json_str)?;
    Ok(())
}
