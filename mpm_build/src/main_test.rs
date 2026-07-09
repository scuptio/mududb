#[cfg(test)]
mod tests {
    #![allow(clippy::panic)] // only for unexpected validation results in tests

    use crate::*;
    use anyhow::Result;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_files(dir: &std::path::Path) -> Result<()> {
        let files = [
            (
                "package.cfg.json",
                "{\"name\":\"test\",\"lang\":\"rust\",\"version\":\"0.1.0\",\"use_async\":true}",
            ),
            (
                "package.desc.json",
                "{\"modules\":{\"test1\":[{\"module_name\":\"test1\",\"proc_name\":\"proc1\",\"param_desc\":{\"fields\":[]},\"return_desc\":{\"fields\":[]}}],\"test2\":[{\"module_name\":\"test2\",\"proc_name\":\"proc2\",\"param_desc\":{\"fields\":[]},\"return_desc\":{\"fields\":[]}}]}}",
            ),
            ("ddl.sql", "CREATE TABLE test (id INT);"),
            ("initdb.sql", "INSERT INTO test VALUES (1);"),
            ("test1.wasm", "mock wasm content"),
            ("test2.wasm", "mock wasm content 2"),
        ];

        for (filename, content) in files {
            let mut file = SFile::create(dir.join(filename))?;
            write!(file, "{}", content)?;
        }

        Ok(())
    }

    fn base_config(temp_dir: &TempDir) -> MpmBuildPackage {
        MpmBuildPackage {
            package_cfg: temp_dir
                .path()
                .join("package.cfg.json")
                .to_string_lossy()
                .into_owned(),
            package_desc: temp_dir
                .path()
                .join("package.desc.json")
                .to_string_lossy()
                .into_owned(),
            ddl_sql: temp_dir
                .path()
                .join("ddl.sql")
                .to_string_lossy()
                .into_owned(),
            initdb_sql: temp_dir
                .path()
                .join("initdb.sql")
                .to_string_lossy()
                .into_owned(),
            wasm_files: vec![
                temp_dir
                    .path()
                    .join("test1.wasm")
                    .to_string_lossy()
                    .into_owned(),
                temp_dir
                    .path()
                    .join("test2.wasm")
                    .to_string_lossy()
                    .into_owned(),
            ],
            output_path: temp_dir
                .path()
                .join("test.mpk")
                .to_string_lossy()
                .into_owned(),
        }
    }

    #[test]
    fn test_package_creation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_test_files(temp_dir.path())?;

        let config = base_config(&temp_dir);
        config.validate()?;
        write_package_archive(&config)?;

        assert!(sync_path_exists(&config.output_path));

        let package_file = SFile::open(&config.output_path)?;
        let mut zip_archive = zip::ZipArchive::new(package_file)?;

        let expected_files = [
            "package.cfg.json",
            "package.desc.json",
            "ddl.sql",
            "initdb.sql",
            "package.manifest.json",
            "test1.wasm",
            "test2.wasm",
        ];

        for expected_file in expected_files {
            assert!(zip_archive.by_name(expected_file).is_ok());
        }

        Ok(())
    }

    #[test]
    fn test_validate_rejects_missing_package_cfg() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_test_files(temp_dir.path())?;

        let mut config = base_config(&temp_dir);
        config.package_cfg = temp_dir
            .path()
            .join("missing.cfg.json")
            .to_string_lossy()
            .into_owned();

        let err = match config.validate() {
            Err(e) => e,
            Ok(_) => panic!("expected validation to fail"),
        };
        assert!(err.to_string().contains("package.cfg.json"));
        Ok(())
    }

    #[test]
    fn test_validate_rejects_missing_package_desc() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_test_files(temp_dir.path())?;

        let mut config = base_config(&temp_dir);
        config.package_desc = temp_dir
            .path()
            .join("missing.desc.json")
            .to_string_lossy()
            .into_owned();

        let err = match config.validate() {
            Err(e) => e,
            Ok(_) => panic!("expected validation to fail"),
        };
        assert!(err.to_string().contains("package.desc.json"));
        Ok(())
    }

    #[test]
    fn test_validate_rejects_missing_ddl_sql() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_test_files(temp_dir.path())?;

        let mut config = base_config(&temp_dir);
        config.ddl_sql = temp_dir
            .path()
            .join("missing.sql")
            .to_string_lossy()
            .into_owned();

        let err = match config.validate() {
            Err(e) => e,
            Ok(_) => panic!("expected validation to fail"),
        };
        assert!(err.to_string().contains("ddl.sql"));
        Ok(())
    }

    #[test]
    fn test_validate_rejects_missing_initdb_sql() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_test_files(temp_dir.path())?;

        let mut config = base_config(&temp_dir);
        config.initdb_sql = temp_dir
            .path()
            .join("missing.sql")
            .to_string_lossy()
            .into_owned();

        let err = match config.validate() {
            Err(e) => e,
            Ok(_) => panic!("expected validation to fail"),
        };
        assert!(err.to_string().contains("initdb.sql"));
        Ok(())
    }

    #[test]
    fn test_validate_rejects_no_wasm_files() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_test_files(temp_dir.path())?;

        let mut config = base_config(&temp_dir);
        config.wasm_files.clear();

        let err = match config.validate() {
            Err(e) => e,
            Ok(_) => panic!("expected validation to fail"),
        };
        assert!(err.to_string().contains("At least one bytecode file"));
        Ok(())
    }

    #[test]
    fn test_validate_rejects_missing_wasm_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_test_files(temp_dir.path())?;

        let mut config = base_config(&temp_dir);
        config.wasm_files.push(
            temp_dir
                .path()
                .join("missing.wasm")
                .to_string_lossy()
                .into_owned(),
        );

        let err = match config.validate() {
            Err(e) => e,
            Ok(_) => panic!("expected validation to fail"),
        };
        assert!(err.to_string().contains("WASM file not found"));
        Ok(())
    }

    #[test]
    fn test_validate_rejects_wasm_file_with_wrong_extension() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_test_files(temp_dir.path())?;
        let bad_path = temp_dir.path().join("test1.txt");
        let mut file = SFile::create(&bad_path)?;
        write!(file, "not wasm")?;

        let mut config = base_config(&temp_dir);
        config.wasm_files = vec![bad_path.to_string_lossy().into_owned()];

        let err = match config.validate() {
            Err(e) => e,
            Ok(_) => panic!("expected validation to fail"),
        };
        assert!(err.to_string().contains(".wasm extension"));
        Ok(())
    }

    #[test]
    fn test_validate_rejects_desc_wasm_module_mismatch() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_test_files(temp_dir.path())?;

        let mut config = base_config(&temp_dir);
        // Keep only test1.wasm so the desc modules no longer match.
        config.wasm_files = vec![config.wasm_files[0].clone()];

        let err = match config.validate() {
            Err(e) => e,
            Ok(_) => panic!("expected validation to fail"),
        };
        assert!(err.to_string().contains("do not match wasm file names"));
        Ok(())
    }

    #[test]
    fn test_create_package_returns_error_for_missing_wasm_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_test_files(temp_dir.path())?;

        let config = base_config(&temp_dir);
        // Remove one of the WASM files that the description expects.
        mudu_sys::fs::sync::sync_remove_file(&config.wasm_files[1])?;

        assert!(write_package_archive(&config).is_err());
        Ok(())
    }

    #[test]
    fn test_add_file_to_zip_returns_error_for_missing_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let output = temp_dir.path().join("test.zip");
        let file = SFile::create(&output)?;
        let mut zip = zip::ZipWriter::new(file);

        assert!(
            add_file_to_zip(&mut zip, temp_dir.path().join("missing.txt"), "missing.txt").is_err()
        );
        Ok(())
    }
}
