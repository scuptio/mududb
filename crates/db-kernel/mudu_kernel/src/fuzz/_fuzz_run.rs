#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]
use crate::fuzz::_golden_corpus::golden_corpus_path;
use lazy_static::lazy_static;
use mudu_sys::env_var;
use mudu_sys::fs::sync::{sync_create_dir_all, sync_write};
use mudu_utils::md5::calc_md5;
use std::collections::HashMap;
use std::path::PathBuf;

type FuzzRun = fn(&[u8]);

// Placeholder for fuzz targets that do not have a concrete implementation yet.
// They are kept in the dispatch table so `cargo fuzz run <target>` does not panic.
fn _unimplemented_fuzz(_data: &[u8]) {}

lazy_static! {
    static ref _FUZZ_RUN: Vec<(&'static str, FuzzRun)> = {
        #[allow(unused_mut)]
        let mut v: Vec<(&'static str, FuzzRun)> = vec![
            (
                "_de_en_x_l_up_tuple",
                crate::contract::xl_d_up_tuple::_fuzz::_de_en_x_l_up_tuple,
            ),
            (
                "_delta_apply",
                crate::common::test_delta_apply::_fuzz::_fuzz_delta_apply,
            ),
            (
                "_de_en_x_l_batch",
                crate::wal::xl_batch::_fuzz::_de_en_x_l_batch,
            ),
            ("_gen_order_csv", crate::test::fuzz_gen_csv::_gen_order_csv),
            ("_type_convert", _unimplemented_fuzz),
            ("_x_log_append", _unimplemented_fuzz),
        ];
        #[cfg(any(test, fuzzing))]
        v.push(("_schema_table", crate::contract::_schema_table));
        v
    };
    static ref FUZZ_RUN: HashMap<&'static str, FuzzRun> = {
        let mut _vec = _FUZZ_RUN.clone();
        let map: HashMap<_, _> = _vec.into_iter().collect();
        map
    };
}

pub fn _target(name: &str, data: &[u8]) {
    let opt = FUZZ_RUN.get(name);
    let f = match opt {
        None => {
            panic!("test {} not found", name);
        }
        Some(f) => f,
    };
    _fuzz_write_data(name, data);
    f(data);
}

fn _fuzz_write_data(name: &str, data: &[u8]) {
    let fuzz_data_dump = env_var::var("GOLDEN_CORPUS").is_some();
    if !fuzz_data_dump {
        return;
    }
    let mut path = PathBuf::from(golden_corpus_path());
    path.push(name);
    if !path.exists() {
        sync_create_dir_all(&path).unwrap();
    }
    let md5 = calc_md5(data);
    path.push(md5);
    sync_write(path, data).unwrap();
}

#[cfg(test)]
mod tests {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    /// Regression test: `_schema_table` must be registered in the fuzz dispatch
    /// table so that `_target("_schema_table", ...)` does not panic with
    /// "test _schema_table not found".
    #[test]
    fn dispatch_table_contains_expected_targets() {
        let names: std::collections::HashSet<&str> =
            super::_FUZZ_RUN.iter().map(|(name, _)| *name).collect();
        assert!(names.contains("_schema_table"));
        assert!(names.contains("_de_en_x_l_up_tuple"));
        assert!(names.contains("_delta_apply"));
        assert!(names.contains("_de_en_x_l_batch"));
        assert!(names.contains("_gen_order_csv"));
        assert!(names.contains("_type_convert"));
        assert!(names.contains("_x_log_append"));
    }

    /// Property/smoke test: every registered fuzz target can consume a small
    /// set of deterministic byte slices without panicking. This gives the
    /// cross-layer serialization/storage paths a baseline regression check
    /// outside of the nightly `cargo fuzz` job.
    ///
    /// Miri is too slow for these cross-layer smoke tests, so run them natively.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn fuzz_targets_handle_sample_inputs_without_panic() {
        let inputs: Vec<&[u8]> = vec![
            &[],
            &[0u8; 64],
            &[0xff; 64],
            b"hello world",
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        ];
        for (name, _) in super::_FUZZ_RUN.iter() {
            // `_gen_order_csv` reads fixture files from `mudu_kernel/data/meta/`
            // and is not suitable for a pure byte-slice smoke test.
            if *name == "_gen_order_csv" {
                continue;
            }
            for input in &inputs {
                let result = catch_unwind(AssertUnwindSafe(|| super::_target(name, input)));
                assert!(
                    result.is_ok(),
                    "fuzz target {name} panicked on input {:?}",
                    input
                );
            }
        }
    }
}
