#![allow(clippy::unwrap_used)]

use super::{FsSync, SFile, SOpenOptions, SyncSysFile};
use std::io::{Read, Seek, SeekFrom, Write};

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
