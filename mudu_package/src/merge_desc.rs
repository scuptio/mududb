use anyhow::Result;
use mudu::utils::json::{read_json, to_json_str};
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::merge_desc_files;
    use anyhow::Result;
    use mudu::utils::json::{read_json, to_json_str};
    use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
    use mudu_contract::procedure::proc_desc::ProcDesc;
    use mudu_contract::tuple::tuple_datum::TupleDatum;
    use std::collections::HashMap;
    use tempfile::tempdir;

    #[test]
    fn merge_desc_files_merges_all_desc_json_files() -> Result<()> {
        let dir = tempdir()?;
        let input = dir.path().join("input");
        mudu_sys::fs::sync::sync_create_dir_all(&input)?;

        let mut modules1 = HashMap::new();
        modules1.insert(
            "mod_a".to_string(),
            vec![ProcDesc::new(
                "mod_a".to_string(),
                "proc_1".to_string(),
                <(i32,)>::tuple_desc_static(&[]),
                <(i64,)>::tuple_desc_static(&[]),
                false,
            )],
        );
        let desc1 = ModProcDesc::new(modules1);
        mudu_sys::fs::sync::sync_write(input.join("one.desc.json"), to_json_str(&desc1)?)?;

        let mut modules2 = HashMap::new();
        modules2.insert(
            "mod_a".to_string(),
            vec![ProcDesc::new(
                "mod_a".to_string(),
                "proc_2".to_string(),
                <(i32, i32)>::tuple_desc_static(&[]),
                <(String,)>::tuple_desc_static(&[]),
                false,
            )],
        );
        modules2.insert(
            "mod_b".to_string(),
            vec![ProcDesc::new(
                "mod_b".to_string(),
                "proc_3".to_string(),
                <(i32,)>::tuple_desc_static(&[]),
                <(i64,)>::tuple_desc_static(&[]),
                false,
            )],
        );
        let desc2 = ModProcDesc::new(modules2);
        mudu_sys::fs::sync::sync_write(input.join("two.desc.json"), to_json_str(&desc2)?)?;

        // Ensure non-desc files are ignored.
        mudu_sys::fs::sync::sync_write(input.join("ignored.json"), "{}")?;

        let output = dir.path().join("merged.desc.json");
        merge_desc_files(&input, &output)?;

        let merged: ModProcDesc = read_json(&output)?;
        let mod_a = merged.modules().get("mod_a").expect("mod_a should exist");
        let mod_b = merged.modules().get("mod_b").expect("mod_b should exist");

        assert_eq!(mod_a.len(), 2);
        assert!(mod_a.iter().any(|p| p.proc_name() == "proc_1"));
        assert!(mod_a.iter().any(|p| p.proc_name() == "proc_2"));
        assert_eq!(mod_b.len(), 1);
        assert_eq!(mod_b[0].proc_name(), "proc_3");
        Ok(())
    }
}
