#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::default_constructed_unit_structs
)]

use super::{FsSync, SFile, SOpenOptions, SyncSysFile};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn temp_dir(label: &str) -> std::path::PathBuf {
    let nanos = crate::time::system_time_now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    crate::env_var::temp_dir().join(format!("mudu-fs-sync-{label}-{nanos}"))
}

fn make_fs() -> FsSync {
    FsSync::new()
}

// Miri does not support the copy_file_range syscall that std::fs::copy uses.
#[cfg_attr(miri, ignore)]
#[test]
fn fs_sync_directory_and_file_lifecycle() {
    let dir = temp_dir("lifecycle");
    let fs = make_fs();
    fs.create_dir_all(&dir).unwrap();

    let file_path = dir.join("hello.txt");
    fs.write(&file_path, b"hello world").unwrap();
    assert!(fs.path_exists(&file_path));

    let bytes = fs.read_all(&file_path).unwrap();
    assert_eq!(bytes, b"hello world");

    let text = fs.read_to_string(&file_path).unwrap();
    assert_eq!(text, "hello world");

    let metadata = fs.metadata(&file_path).unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.len(), 11);
    assert_eq!(fs.metadata_len(&file_path).unwrap(), 11);

    let dest = dir.join("copy.txt");
    let copied = fs.copy(&file_path, &dest).unwrap();
    assert_eq!(copied, 11);
    assert_eq!(fs.read_all(&dest).unwrap(), b"hello world");

    fs.remove_file(&file_path).unwrap();
    assert!(!fs.path_exists(&file_path));

    // removing a non-existent file is a no-op.
    assert!(fs.remove_file(&file_path).is_ok());

    fs.remove_dir_all(&dir).unwrap();
}

#[test]
fn fs_sync_read_dir_and_entries() {
    let dir = temp_dir("readdir");
    let fs = make_fs();
    fs.create_dir_all(&dir).unwrap();
    fs.write(&dir.join("a.txt"), b"a").unwrap();
    fs.write(&dir.join("b.txt"), b"b").unwrap();

    let paths = fs.read_dir(&dir).unwrap();
    assert_eq!(paths.len(), 2);

    let entries = fs.read_dir_entries(&dir).unwrap();
    assert_eq!(entries.len(), 2);
    let mut names: Vec<_> = entries.iter().map(|e| e.file_name()).collect();
    names.sort();
    assert_eq!(names, vec!["a.txt", "b.txt"]);

    let file_type = entries[0].file_type().unwrap();
    assert!(file_type.is_file());
    let meta = entries[0].metadata().unwrap();
    assert!(meta.is_file());

    fs.remove_dir_all(&dir).unwrap();
}

#[test]
fn sync_sys_file_read_write_and_metadata() {
    let dir = temp_dir("sysfile");
    let fs = make_fs();
    fs.create_dir_all(&dir).unwrap();
    let path = dir.join("data.bin");

    let file: SyncSysFile = fs
        .open_sys_file_with_options(
            &path,
            std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .read(true),
        )
        .unwrap();

    file.write_all_at(0, b"0123456789").unwrap();
    file.fsync().unwrap();
    assert_eq!(file.file_len().unwrap(), 10);

    let buf = file.read_exact_at(2, 4).unwrap();
    assert_eq!(buf, b"2345");

    assert!(file.close().is_ok());

    #[cfg(unix)]
    {
        let file2: SyncSysFile = fs.open_sys_file(&path).unwrap();
        assert!(file2.as_raw_fd().is_some());
    }

    fs.remove_dir_all(&dir).unwrap();
}

// Miri does not support fcntl(F_GETFL) used by std::fs::File's Debug impl.
#[cfg_attr(miri, ignore)]
#[test]
fn sfile_open_create_and_io_traits() {
    let dir = temp_dir("sfile");
    let fs = make_fs();
    fs.create_dir_all(&dir).unwrap();
    let path = dir.join("sfile.txt");

    {
        let mut file = SFile::create(&path).unwrap();
        file.write_all(b"hello").unwrap();
        file.flush().unwrap();
        file.sync_all().unwrap();
        file.sync_data().unwrap();
        file.set_len(3).unwrap();
    }

    let meta = SFile::open(&path).unwrap().metadata().unwrap();
    assert_eq!(meta.len(), 3);

    {
        let mut file = SFile::open(&path).unwrap();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, b"hel");
        file.seek(SeekFrom::Start(0)).unwrap();
    }

    let cloned = SFile::open(&path).unwrap().try_clone().unwrap();
    let _ = cloned.metadata().unwrap();

    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let file = SFile::open(&path).unwrap();
        assert!(file.as_raw_fd() >= 0);
    }

    let debug = format!("{:?}", SFile::open(&path).unwrap());
    assert!(debug.contains("File"));

    fs.remove_dir_all(&dir).unwrap();
}

#[test]
fn sopen_options_builds_and_opens() {
    let dir = temp_dir("openoptions");
    let fs = make_fs();
    fs.create_dir_all(&dir).unwrap();
    let path = dir.join("opts.txt");

    let mut opts = SOpenOptions::new();
    opts.read(true).write(true).create(true).truncate(true);
    let mut file = opts.open(&path).unwrap();
    file.write_all(b"opts").unwrap();
    drop(file);

    assert_eq!(fs.read_all(&path).unwrap(), b"opts");

    let mut default: SOpenOptions = Default::default();
    default.read(true);
    assert!(default.open(&path).is_ok());

    fs.remove_dir_all(&dir).unwrap();
}

#[test]
fn metadata_and_dir_entry_debug() {
    let dir = temp_dir("meta-debug");
    let fs = make_fs();
    fs.create_dir_all(&dir).unwrap();
    let path = dir.join("file.txt");
    fs.write(&path, b"x").unwrap();

    let meta = fs.metadata(&path).unwrap();
    let debug = format!("{:?}", meta);
    assert!(debug.contains("FileAttr") || debug.contains("Metadata"));

    let entries = fs.read_dir_entries(&dir).unwrap();
    let debug = format!("{:?}", entries[0].file_type().unwrap());
    assert!(debug.contains("FileType") || debug.contains("FileAttr"));

    fs.remove_dir_all(&dir).unwrap();
}

fn tmp_dir() -> PathBuf {
    let dir = PathBuf::from("target/tmp").join(format!("sync-fs-{}", Uuid::new_v4()));
    let fs = FsSync::new();
    fs.create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(dir: &Path) {
    let fs = FsSync::new();
    let _ = fs.remove_dir_all(dir);
}

#[test]
fn fs_sync_new_and_default() {
    let _ = FsSync::new();
    let _ = FsSync::default();
}

#[test]
fn fs_sync_create_remove_dir_all_and_path_exists() {
    let fs = FsSync::new();
    let dir = tmp_dir();
    assert!(fs.path_exists(&dir));
    fs.remove_dir_all(&dir).unwrap();
    assert!(!fs.path_exists(&dir));
}

#[test]
fn fs_sync_write_read_all_read_to_string_roundtrip() {
    let fs = FsSync::new();
    let dir = tmp_dir();
    let path = dir.join("file.txt");
    fs.write(&path, b"hello world").unwrap();
    assert_eq!(fs.read_all(&path).unwrap(), b"hello world");
    assert_eq!(fs.read_to_string(&path).unwrap(), "hello world");
    cleanup(&dir);
}

#[test]
fn fs_sync_metadata_len_and_metadata() {
    let fs = FsSync::new();
    let dir = tmp_dir();
    let file = dir.join("data.bin");
    fs.write(&file, b"payload").unwrap();
    assert_eq!(fs.metadata_len(&file).unwrap(), 7);
    let meta = fs.metadata(&file).unwrap();
    assert!(meta.is_file());
    assert!(!meta.is_dir());
    assert!(!meta.is_empty());
    assert_eq!(meta.len(), 7);

    let empty = dir.join("empty");
    fs.write(&empty, b"").unwrap();
    assert!(fs.metadata(&empty).unwrap().is_empty());

    let subdir = dir.join("subdir");
    fs.create_dir_all(&subdir).unwrap();
    let dir_meta = fs.metadata(&subdir).unwrap();
    assert!(dir_meta.is_dir());
    assert!(!dir_meta.is_file());
    cleanup(&dir);
}

#[test]
fn fs_sync_read_dir_and_read_dir_entries() {
    let fs = FsSync::new();
    let dir = tmp_dir();
    fs.write(&dir.join("a.txt"), b"a").unwrap();
    fs.write(&dir.join("b.txt"), b"b").unwrap();

    let paths = fs.read_dir(&dir).unwrap();
    assert_eq!(paths.len(), 2);
    assert!(paths.iter().any(|p| p.file_name().unwrap() == "a.txt"));
    assert!(paths.iter().any(|p| p.file_name().unwrap() == "b.txt"));

    let entries = fs.read_dir_entries(&dir).unwrap();
    assert_eq!(entries.len(), 2);
    let mut names: Vec<_> = entries.iter().map(|e| e.file_name()).collect();
    names.sort();
    assert_eq!(names, vec!["a.txt", "b.txt"]);
    for entry in &entries {
        assert!(entry.path().starts_with(&dir));
        assert!(entry.file_type().unwrap().is_file());
        assert!(entry.metadata().unwrap().is_file());
    }
    cleanup(&dir);
}

#[test]
fn fs_sync_remove_file_idempotent() {
    let fs = FsSync::new();
    let dir = tmp_dir();
    let path = dir.join("to-remove.txt");
    fs.remove_file(&path).unwrap();
    fs.write(&path, b"x").unwrap();
    fs.remove_file(&path).unwrap();
    assert!(!fs.path_exists(&path));
    fs.remove_file(&path).unwrap();
    cleanup(&dir);
}

// Miri does not support the copy_file_range syscall that std::fs::copy uses.
#[cfg_attr(miri, ignore)]
#[test]
fn fs_sync_copy_file() {
    let fs = FsSync::new();
    let dir = tmp_dir();
    let src = dir.join("src.txt");
    let dst = dir.join("dst.txt");
    fs.write(&src, b"copy me").unwrap();
    let copied = fs.copy(&src, &dst).unwrap();
    assert_eq!(copied, 7);
    assert_eq!(fs.read_all(&dst).unwrap(), b"copy me");
    cleanup(&dir);
}

#[test]
fn fs_sync_open_create_and_options() {
    let fs = FsSync::new();
    let dir = tmp_dir();
    let path = dir.join("created.txt");
    {
        let mut file = fs.create(&path).unwrap();
        file.write_all(b"created").unwrap();
    }
    {
        let mut file = fs.open(&path).unwrap();
        let mut buf = String::new();
        file.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "created");
    }

    let path2 = dir.join("options.txt");
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(true);
    let mut file = fs.open_with_options(&path2, &opts).unwrap();
    file.write_all(b"options").unwrap();
    drop(file);
    assert_eq!(fs.read_all(&path2).unwrap(), b"options");

    let mut sopts = SOpenOptions::new();
    sopts.read(true).write(true).create(true).truncate(true);
    let mut file = sopts.open(dir.join("soptions.txt")).unwrap();
    file.write_all(b"soptions").unwrap();
    drop(file);
    assert_eq!(fs.read_all(&dir.join("soptions.txt")).unwrap(), b"soptions");
    cleanup(&dir);
}

#[test]
fn sync_sys_file_positioned_io() {
    let fs = FsSync::new();
    let dir = tmp_dir();
    let path = dir.join("sysfile.bin");
    let mut std_opts = std::fs::OpenOptions::new();
    std_opts.read(true).write(true).create(true).truncate(true);
    let sys_file = fs.open_sys_file_with_options(&path, &std_opts).unwrap();
    sys_file.write_all_at(0, b"hello").unwrap();
    sys_file.write_all_at(5, b" world").unwrap();
    sys_file.fsync().unwrap();
    assert_eq!(sys_file.file_len().unwrap(), 11);
    assert_eq!(sys_file.read_exact_at(0, 5).unwrap(), b"hello");
    assert_eq!(sys_file.read_exact_at(6, 5).unwrap(), b"world");
    sys_file.close().unwrap();
    cleanup(&dir);
}

#[test]
fn sfile_read_write_seek() {
    let dir = tmp_dir();
    let path = dir.join("sfile.bin");
    {
        let mut file = SFile::create(&path).unwrap();
        file.write_all(b"abc").unwrap();
        file.write_all(b"def").unwrap();
    }
    {
        let mut file = SFile::open(&path).unwrap();
        file.seek(SeekFrom::Start(3)).unwrap();
        let mut buf = [0u8; 3];
        file.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"def");
    }
    cleanup(&dir);
}

#[test]
fn sfile_sync_all_sync_data_set_len_try_clone() {
    let dir = tmp_dir();
    let path = dir.join("sfile2.bin");
    let mut file = SFile::create(&path).unwrap();
    file.write_all(b"0123456789").unwrap();
    file.sync_all().unwrap();
    file.sync_data().unwrap();
    file.set_len(4).unwrap();
    drop(file);
    assert_eq!(FsSync::new().metadata_len(&path).unwrap(), 4);

    let file = SFile::open(&path).unwrap();
    let clone = file.try_clone().unwrap();
    drop(file);
    let mut buf = String::new();
    let mut clone = clone;
    clone.read_to_string(&mut buf).unwrap();
    assert_eq!(buf, "0123");
    cleanup(&dir);
}

#[test]
fn sopen_options_builder_chain() {
    let dir = tmp_dir();
    let path = dir.join("chain.txt");
    let mut file = SOpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .unwrap();
    file.write_all(b"chain").unwrap();
    drop(file);
    assert_eq!(FsSync::new().read_all(&path).unwrap(), b"chain");
    cleanup(&dir);
}

#[test]
fn smetadata_sfile_type_sdirentry_behavior() {
    let fs = FsSync::new();
    let dir = tmp_dir();
    let file = dir.join("meta.txt");
    fs.write(&file, b"data").unwrap();

    let meta = fs.metadata(&file).unwrap();
    assert!(meta.is_file());
    assert!(!meta.is_dir());
    assert!(!meta.is_symlink());
    assert_eq!(meta.len(), 4);

    let entries = fs.read_dir_entries(&dir).unwrap();
    let entry = entries.into_iter().next().unwrap();
    let ft = entry.file_type().unwrap();
    assert!(ft.is_file());
    assert!(!ft.is_dir());
    assert!(!ft.is_symlink());
    let entry_meta = entry.metadata().unwrap();
    assert!(entry_meta.is_file());
    cleanup(&dir);
}

// Miri does not support the copy_file_range syscall that std::fs::copy uses.
#[cfg_attr(miri, ignore)]
#[test]
fn top_level_helpers_exposed() {
    let dir = tmp_dir();
    let path = dir.join("top.txt");
    super::create_dir_all(dir.join("nested")).unwrap();
    super::write(&path, b"top").unwrap();
    assert!(super::path_exists(&path));
    assert_eq!(super::read(&path).unwrap(), b"top");
    assert_eq!(super::read_to_string(&path).unwrap(), "top");
    let copied_path = dir.join("top-copy.txt");
    assert_eq!(super::copy(&path, &copied_path).unwrap(), 3);
    let meta = super::metadata(&path).unwrap();
    assert!(meta.is_file());
    let paths = super::read_dir(&dir).unwrap();
    assert_eq!(paths.len(), 3);
    let entries = super::read_dir_entries(&dir).unwrap();
    assert_eq!(entries.len(), 3);

    let opt_path = dir.join("opt.bin");
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(true);
    let file = super::open_with_options(&opt_path, &opts).unwrap();
    file.write_all_at(0, b"opts").unwrap();
    drop(file);
    let file = super::open(&opt_path).unwrap();
    let buf = file.read_exact_at(0, 4).unwrap();
    assert_eq!(buf, b"opts");

    super::remove_file(&path).unwrap();
    super::remove_dir_all(dir.join("nested")).unwrap();
    super::remove_dir_all(&dir).unwrap();
    assert!(!super::path_exists(&dir));
    cleanup(&dir);
}
