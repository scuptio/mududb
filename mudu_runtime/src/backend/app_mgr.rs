use crate::backend::mudu_app_mgr::ListOption;
use crate::backend::mududb_cfg::MuduDBCfg;
use crate::service::app_list::AppList;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_kernel::server::async_func_runtime::AsyncFuncInvoker;
use std::sync::Arc;

#[async_trait(?Send)]
pub trait AppMgr: Send + Sync {
    /// Install one application package from its `.mpk` binary payload.
    ///
    /// The payload is the exact package file contents, not a filesystem path.
    /// Implementations should reuse the existing runtime/package installation
    /// flow instead of inventing a parallel format or deployment mechanism, so
    /// package validation and compatibility semantics stay identical to the
    /// legacy runtime path.
    async fn install(&self, mpk_binary: Vec<u8>) -> RS<()>;

    /// Remove one installed application by application name.
    ///
    /// The input is a UTF-8 encoded application name. The implementation is
    /// expected to remove the package from the manager's visible application
    /// set while preserving the behavior of all existing runtime interfaces.
    /// If no such application exists, an error should be returned.
    async fn uninstall(&self, app_name: Vec<u8>) -> RS<()>;

    /// Return application metadata according to the supplied filter options.
    ///
    /// This method should expose the same package-derived information that the
    /// existing runtime can already observe, such as application info, DDL, and
    /// procedure/module descriptions. It should not require callers to know
    /// anything about the internal package layout on disk.
    async fn list(&self, option: &ListOption) -> RS<AppList>;

    /// Create a new procedure invoker for one runtime consumer.
    ///
    /// This is a factory method, not a shared accessor. Each returned
    /// `Arc<dyn AsyncFuncInvoker>` must own an independent invocation environment and
    /// must not share mutable runtime internals, stores, or other execution
    /// state with invokers returned from other calls. This requirement exists
    /// so backends such as the io_uring worker runtime can safely create one
    /// invoker per worker thread.
    ///
    /// The implementation must reuse the current procedure invocation
    /// mechanism, and it must not change or bypass any of the existing public
    /// runtime behavior for legacy `p1` or component-based execution.
    async fn create_invoker(&self, cfg: &MuduDBCfg) -> RS<Arc<dyn AsyncFuncInvoker>>;
}
