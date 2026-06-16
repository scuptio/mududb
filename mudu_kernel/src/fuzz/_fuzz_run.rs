use crate::fuzz::_golden_corpus::golden_corpus_path;
use lazy_static::lazy_static;
use mudu_sys::fs::sync::{sync_create_dir_all, sync_write};
use mudu_sys::env_var;
use mudu_utils::md5::calc_md5;
use std::collections::HashMap;
use std::path::PathBuf;

type FuzzRun = fn(&[u8]);

lazy_static! {
    static ref _FUZZ_RUN: Vec<(&'static str, FuzzRun)> = vec![
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
        ("_gen_order_csv", crate::test::fuzz_gen_csv::_gen_order_csv,),
    ];
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
