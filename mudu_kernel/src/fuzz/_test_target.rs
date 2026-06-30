#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]
#[cfg(test)]
pub mod _test {
    use crate::fuzz::_fuzz_run::_target;
    use crate::fuzz::_golden_corpus::golden_corpus_path;
    use mudu_utils::log::log_setup;
    use std::path::PathBuf;
    use tracing::debug;

    pub fn _test_target(name: &str) {
        log_setup("info");
        __test_target(name);
        debug!("{} test success", name);
    }

    fn __test_target(name: &str) {
        let s = golden_corpus_path();
        let path = PathBuf::from(s).join(name);
        for entry in mudu_sys::fs::sync::read_dir_entries(path.as_path()).unwrap() {
            if entry.path().is_file() {
                //info!("Testing {}", entry.file_name().to_str().unwrap());
                let data = mudu_sys::fs::sync::read(entry.path()).unwrap();
                _target(name, data.as_slice())
            }
        }
    }
}
