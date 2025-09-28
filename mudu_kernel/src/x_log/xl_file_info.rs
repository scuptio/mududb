use crate::x_log::xl_cfg::XLCfg;

#[derive(Clone, Debug)]
pub struct XLFileInfo {
    pub cfg: XLCfg,
    pub file_no: u32,
    pub file_size: u64,
    pub channel_name: String,
}
