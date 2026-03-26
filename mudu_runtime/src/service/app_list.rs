use mudu::common::app_info::AppInfo;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct AppList {
    pub apps: Vec<AppListItem>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AppListItem {
    pub info: AppInfo,
    pub ddl: String,
    pub mod_proc_desc: ModProcDesc,
}
