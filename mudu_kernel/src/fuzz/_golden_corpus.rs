use lazy_static::lazy_static;
use mudu::this_file;
use std::path::PathBuf;

lazy_static! {
    static ref GOLDEN_CORPUS_PATH: String = {
        let _p = PathBuf::from(this_file!());
        let _p = _p.parent().unwrap().parent().unwrap().parent().unwrap();
        let _p = _p.join("fuzz").join("golden_corpus");
        _p.as_path().to_str().unwrap().to_string()
    };
}

pub fn golden_corpus_path() -> String {
    GOLDEN_CORPUS_PATH.clone()
}
