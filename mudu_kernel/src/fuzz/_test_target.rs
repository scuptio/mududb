#[cfg(test)]
pub mod _test {
    use crate::fuzz::_fuzz_run::_target;
    use crate::fuzz::_golden_corpus::golden_corpus_path;
    use mudu_utils::log::log_setup;
    use std::fs;
    use std::path::PathBuf;
    use tracing::info;

    pub fn _test_target(name: &str) {
        log_setup("info");
        __test_target(name);
        info!("{} test success", name);
    }

    fn __test_target(name: &str) {
        let s = golden_corpus_path();
        let path = PathBuf::from(s).join(name);
        for r_entry in fs::read_dir(path.as_path()).unwrap() {
            let entry = r_entry.unwrap();
            if entry.path().is_file() {
                info!("Testing {}", entry.file_name().to_str().unwrap());
                let data = fs::read(entry.path()).unwrap();
                _target(name, data.as_slice())
            }
        }
    }
}
