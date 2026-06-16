#[derive(Debug, Clone, Copy, Default)]
pub struct FileOptions {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub truncate: bool,
    pub append: bool,
    pub create_new: bool,
    pub mode: u32,
}

impl FileOptions {
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

    pub fn new(flags: i32, mode: u32) -> Self {
        let mut opts = Self::default();
        opts.set_flags(flags);
        opts.set_mode(mode);
        opts
    }

    pub fn set_flags(&mut self, flags: i32) {
        let read = (flags & libc::O_RDWR) != 0 || (flags & libc::O_WRONLY) == 0;
        let write = (flags & libc::O_RDWR) != 0 || (flags & libc::O_WRONLY) != 0;
        self.read = read;
        self.write = write;
        self.create = (flags & libc::O_CREAT) != 0;
        self.truncate = (flags & libc::O_TRUNC) != 0;
        self.append = (flags & libc::O_APPEND) != 0;
    }

    pub fn set_mode(&mut self, mode: u32) {
        self.mode = mode;
    }
}
