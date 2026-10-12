//! Metadata describing a guest application package.

use serde::{Deserialize, Serialize};

/// Identity and runtime properties of a guest application.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppInfo {
    /// Application name.
    pub name: String,
    /// Implementation language of the application.
    pub lang: String,
    /// Application version string.
    pub version: String,
    /// Whether the application uses asynchronous procedures.
    pub use_async: bool,
}
