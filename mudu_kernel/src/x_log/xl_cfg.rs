impl XLCfg {
    pub fn new(
        path: String,
        ext: String,
        x_log_channels: u32,
        x_log_file_size_limit: u64,
        use_io_uring: bool,
    ) -> XLCfg {
        Self {
            x_log_use_io_uring: use_io_uring,
            x_log_path: path,
            x_log_ext_name: ext,
            x_log_channels,
            x_log_file_size_limit,
        }
    }
}

#[derive(Clone, Debug)]
pub struct XLCfg {
    pub x_log_use_io_uring: bool,
    pub x_log_path: String,
    pub x_log_ext_name: String,
    pub x_log_channels: u32,
    pub x_log_file_size_limit: u64,
}

impl XLCfg {
    pub fn log_path(&self) -> &String {
        &self.x_log_path
    }

    pub fn log_ext_name(&self) -> &String {
        &self.x_log_ext_name
    }

    pub fn log_channels(&self) -> u32 {
        self.x_log_channels
    }

    pub fn log_file_size_limit(&self) -> u64 {
        self.x_log_file_size_limit
    }
}
