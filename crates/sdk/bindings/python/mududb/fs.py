"""Canonical `mududb.fs` segment: filesystem operations.

Re-exports the fs syscall family from `mududb.sys` plus the flag/whence
constants — the deployed wire values shared with the AssemblyScript and
Rust bindings.
"""

from mududb.sys import (
    fs_close,
    fs_fstat,
    fs_fsync,
    fs_lseek,
    fs_open,
    fs_pread,
    fs_pwrite,
    fs_read,
    fs_readdir,
    fs_stat,
    fs_write,
)

__all__ = [
    "FS_O_RDONLY",
    "FS_O_WRONLY",
    "FS_O_RDWR",
    "FS_SEEK_SET",
    "FS_SEEK_CUR",
    "FS_SEEK_END",
    "fs_open",
    "fs_close",
    "fs_read",
    "fs_write",
    "fs_pread",
    "fs_pwrite",
    "fs_lseek",
    "fs_fstat",
    "fs_stat",
    "fs_fsync",
    "fs_readdir",
]

# Open access modes.
FS_O_RDONLY = 0
FS_O_WRONLY = 1
FS_O_RDWR = 2

# Seek whence values.
FS_SEEK_SET = 0
FS_SEEK_CUR = 1
FS_SEEK_END = 2
