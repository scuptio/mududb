use mudu::common::app_info::AppInfo;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use serde::{Deserialize, Serialize};

/// List of installed Mudu applications.
#[derive(Serialize, Deserialize, Clone)]
pub struct AppList {
    /// Items in the list.
    pub apps: Vec<AppListItem>,
}

/// Metadata describing one installed application.
#[derive(Serialize, Deserialize, Clone)]
pub struct AppListItem {
    /// Application information.
    pub info: AppInfo,
    /// DDL text defining the application schema.
    pub ddl: String,
    /// Module and procedure descriptors.
    pub mod_proc_desc: ModProcDesc,
}
