#[cfg(test)]
mod tests {
    #![allow(clippy::panic)] // only for missing-module assertions in tests

    use crate::merge_desc::merge_desc_files;
    use anyhow::Result;
    use mudu::utils::json::to_json_str;
    use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
    use mudu_contract::procedure::proc_desc::ProcDesc;
    use mudu_contract::tuple::tuple_datum::TupleDatum;
    use mudu_utils::json::read_json;
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn sample_proc(module: &str, proc: &str) -> ProcDesc {
        ProcDesc::new(
            module.to_string(),
            proc.to_string(),
            <(i32,)>::tuple_desc_static(&[]),
            <(i64,)>::tuple_desc_static(&[]),
            false,
        )
    }

    #[test]
    fn merge_desc_files_merges_all_desc_json_files() -> Result<()> {
        let dir = tempdir()?;
        let input = dir.path().join("input");
        mudu_sys::fs::sync::sync_create_dir_all(&input)?;

        let mut modules1 = HashMap::new();
        modules1.insert("mod_a".to_string(), vec![sample_proc("mod_a", "proc_1")]);
        let desc1 = ModProcDesc::new(modules1);
        mudu_sys::fs::sync::sync_write(input.join("one.desc.json"), to_json_str(&desc1)?)?;

        let mut modules2 = HashMap::new();
        modules2.insert("mod_a".to_string(), vec![sample_proc("mod_a", "proc_2")]);
        modules2.insert("mod_b".to_string(), vec![sample_proc("mod_b", "proc_3")]);
        let desc2 = ModProcDesc::new(modules2);
        mudu_sys::fs::sync::sync_write(input.join("two.desc.json"), to_json_str(&desc2)?)?;

        // Ensure non-desc files are ignored.
        mudu_sys::fs::sync::sync_write(input.join("ignored.json"), "{}")?;

        let output = dir.path().join("merged.desc.json");
        merge_desc_files(&input, &output)?;

        let merged: ModProcDesc = read_json(&output)?;
        let mod_a = match merged.modules().get("mod_a") {
            Some(m) => m,
            None => panic!("mod_a should exist"),
        };
        let mod_b = match merged.modules().get("mod_b") {
            Some(m) => m,
            None => panic!("mod_b should exist"),
        };

        assert_eq!(mod_a.len(), 2);
        assert!(mod_a.iter().any(|p| p.proc_name() == "proc_1"));
        assert!(mod_a.iter().any(|p| p.proc_name() == "proc_2"));
        assert_eq!(mod_b.len(), 1);
        assert_eq!(mod_b[0].proc_name(), "proc_3");
        Ok(())
    }

    #[test]
    fn merge_desc_files_empty_folder_produces_empty_desc() -> Result<()> {
        let dir = tempdir()?;
        let input = dir.path().join("input");
        mudu_sys::fs::sync::sync_create_dir_all(&input)?;

        let output = dir.path().join("merged.desc.json");
        merge_desc_files(&input, &output)?;

        let merged: ModProcDesc = read_json(&output)?;
        assert!(merged.modules().is_empty());
        Ok(())
    }

    #[test]
    fn merge_desc_files_ignores_non_desc_files_and_subdirectories() -> Result<()> {
        let dir = tempdir()?;
        let input = dir.path().join("input");
        mudu_sys::fs::sync::sync_create_dir_all(&input)?;
        mudu_sys::fs::sync::sync_create_dir_all(input.join("sub"))?;
        mudu_sys::fs::sync::sync_write(input.join("notes.txt"), "hello")?;
        mudu_sys::fs::sync::sync_write(input.join("data.json"), "{}")?;

        let output = dir.path().join("merged.desc.json");
        merge_desc_files(&input, &output)?;

        let merged: ModProcDesc = read_json(&output)?;
        assert!(merged.modules().is_empty());
        Ok(())
    }

    #[test]
    fn merge_desc_files_matches_extension_case_insensitively() -> Result<()> {
        let dir = tempdir()?;
        let input = dir.path().join("input");
        mudu_sys::fs::sync::sync_create_dir_all(&input)?;

        let mut modules = HashMap::new();
        modules.insert("mod_c".to_string(), vec![sample_proc("mod_c", "proc_1")]);
        let desc = ModProcDesc::new(modules);
        mudu_sys::fs::sync::sync_write(input.join("caps.DESC.JSON"), to_json_str(&desc)?)?;
        mudu_sys::fs::sync::sync_write(input.join("mixed.Desc.Json"), to_json_str(&desc)?)?;

        let output = dir.path().join("merged.desc.json");
        merge_desc_files(&input, &output)?;

        let merged: ModProcDesc = read_json(&output)?;
        let mod_c = match merged.modules().get("mod_c") {
            Some(m) => m,
            None => panic!("mod_c should exist"),
        };
        assert_eq!(mod_c.len(), 2);
        Ok(())
    }

    #[test]
    fn merge_desc_files_appends_duplicate_module_entries() -> Result<()> {
        let dir = tempdir()?;
        let input = dir.path().join("input");
        mudu_sys::fs::sync::sync_create_dir_all(&input)?;

        let mut modules = HashMap::new();
        modules.insert("dup".to_string(), vec![sample_proc("dup", "proc_1")]);
        let desc = ModProcDesc::new(modules);
        mudu_sys::fs::sync::sync_write(input.join("first.desc.json"), to_json_str(&desc)?)?;
        mudu_sys::fs::sync::sync_write(input.join("second.desc.json"), to_json_str(&desc)?)?;

        let output = dir.path().join("merged.desc.json");
        merge_desc_files(&input, &output)?;

        let merged: ModProcDesc = read_json(&output)?;
        let dup = match merged.modules().get("dup") {
            Some(m) => m,
            None => panic!("dup should exist"),
        };
        assert_eq!(dup.len(), 2);
        assert!(dup.iter().all(|p| p.proc_name() == "proc_1"));
        Ok(())
    }

    #[test]
    fn merge_desc_files_overwrites_existing_output() -> Result<()> {
        let dir = tempdir()?;
        let input = dir.path().join("input");
        mudu_sys::fs::sync::sync_create_dir_all(&input)?;

        let mut modules = HashMap::new();
        modules.insert("mod_x".to_string(), vec![sample_proc("mod_x", "proc_x")]);
        let desc = ModProcDesc::new(modules);
        mudu_sys::fs::sync::sync_write(input.join("x.desc.json"), to_json_str(&desc)?)?;

        let output = dir.path().join("merged.desc.json");
        mudu_sys::fs::sync::sync_write(&output, "stale content")?;

        merge_desc_files(&input, &output)?;

        let merged: ModProcDesc = read_json(&output)?;
        assert!(merged.modules().contains_key("mod_x"));
        Ok(())
    }

    #[test]
    fn merge_desc_files_returns_error_for_invalid_desc_json() -> Result<()> {
        let dir = tempdir()?;
        let input = dir.path().join("input");
        mudu_sys::fs::sync::sync_create_dir_all(&input)?;
        mudu_sys::fs::sync::sync_write(input.join("bad.desc.json"), "not valid json")?;

        let output = dir.path().join("merged.desc.json");
        assert!(merge_desc_files(&input, &output).is_err());
        Ok(())
    }

    #[test]
    fn merge_desc_files_returns_error_for_missing_input_folder() -> Result<()> {
        let dir = tempdir()?;
        let input = dir.path().join("does_not_exist");
        let output = dir.path().join("merged.desc.json");
        assert!(merge_desc_files(&input, &output).is_err());
        Ok(())
    }
}
