#[derive(Debug, Clone, Copy, Default)]
/// Options controlling how a file is opened.
pub struct FileOptions {
    /// Open for reading.
    pub read: bool,
    /// Open for writing.
    pub write: bool,
    /// Create the file if it does not exist.
    pub create: bool,
    /// Truncate the file to zero length.
    pub truncate: bool,
    /// Open in append mode.
    pub append: bool,
    /// Fail if the file already exists.
    pub create_new: bool,
    /// Permission bits used when creating the file.
    pub mode: u32,
}

impl FileOptions {
    /// Options for a read-only open.
    pub const fn read_only() -> Self {
        Self {
            read: true,
            write: false,
            create: false,
            truncate: false,
            append: false,
            create_new: false,
            mode: 0,
        }
    }

    /// Options for reading, writing and creating a file.
    pub const fn read_write_create() -> Self {
        Self {
            read: true,
            write: true,
            create: true,
            truncate: false,
            append: false,
            create_new: false,
            mode: 0,
        }
    }

    /// Build options from raw `libc` open flags and permission mode.
    pub fn new(flags: i32, mode: u32) -> Self {
        let mut opts = Self::default();
        opts.set_flags(flags);
        opts.set_mode(mode);
        opts
    }

    /// Apply raw `libc` open flags.
    pub fn set_flags(&mut self, flags: i32) {
        let read = (flags & libc::O_RDWR) != 0 || (flags & libc::O_WRONLY) == 0;
        let write = (flags & libc::O_RDWR) != 0 || (flags & libc::O_WRONLY) != 0;
        self.read = read;
        self.write = write;
        self.create = (flags & libc::O_CREAT) != 0;
        self.truncate = (flags & libc::O_TRUNC) != 0;
        self.append = (flags & libc::O_APPEND) != 0;
    }

    /// Set the permission mode used when creating the file.
    pub fn set_mode(&mut self, mode: u32) {
        self.mode = mode;
    }
}
