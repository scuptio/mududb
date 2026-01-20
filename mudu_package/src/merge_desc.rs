use anyhow::Result;
use mudu::utils::json::{read_json, to_json_str};
use mudu_contract::procedure::package_desc::PackageDesc;
use std::fs;
use std::path::Path;

pub fn merge_desc_files<P: AsRef<Path>, D: AsRef<Path>>(
    input_folder: P,
    output_desc_file: D,
) -> Result<()> {
    let mut package_desc = PackageDesc::new(Default::default());
    let dir = fs::read_dir(input_folder.as_ref())?;
    for r_entry in dir {
        let entry = r_entry?;
        let meta = entry.metadata()?;
        if meta.is_file() {
            let s = entry.file_name().to_string_lossy().to_string();
            if s.to_lowercase().ends_with(".desc.json") {
                let mut d = read_json::<PackageDesc, &Path>(entry.path().as_ref())?;
                package_desc.merge(&mut d);
            }
        }
    }

    let json_str = to_json_str(&package_desc)?;
    fs::write(output_desc_file, json_str)?;
    Ok(())
}
