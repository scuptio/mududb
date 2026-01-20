#[derive(Clone)]
pub struct StorageCfg {
    pub buffer_num_pages: u64,
    pub page_size:u64,
    pub extent_pages:u64,
    pub path:String,
}