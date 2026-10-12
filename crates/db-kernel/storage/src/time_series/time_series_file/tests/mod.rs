use project_root::get_project_root;

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod basic;
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod batch_integrity;
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod pages_concurrency;
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod wal_recovery;

fn temp_ts_path(name: &str) -> std::path::PathBuf {
    let root = get_project_root().unwrap();
    root.join("target").join("tmp").join(format!(
        "tsf-{}-{}.dat",
        name,
        mudu_sys::random::uuid_v4()
    ))
}

fn temp_relation_base(name: &str) -> std::path::PathBuf {
    let root = get_project_root().unwrap();
    root.join("target").join("tmp").join(format!(
        "tsf-rel-{}-{}",
        name,
        mudu_sys::random::uuid_v4()
    ))
}

fn payload(byte: u8, len: usize) -> Vec<u8> {
    vec![byte; len]
}
