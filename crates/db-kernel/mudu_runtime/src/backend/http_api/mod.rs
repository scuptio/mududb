mod http_api_capabilities;
pub use http_api_capabilities::HttpApiCapabilities;

mod procedure_list;

mod http_api_context;
pub(crate) use http_api_context::HttpApiContext;

mod legacy_http_api;
pub use legacy_http_api::LegacyHttpApi;

mod kernel_http_api;
mod kernel_invoke_client;
mod kernel_invoke_client_factory;
pub use kernel_http_api::KernelHttpApi;
pub use kernel_invoke_client_factory::KernelInvokeClientFactory;

mod dto;
pub use dto::*;

mod handlers;
mod invoke_util;
mod serve;

#[cfg(test)]
pub(crate) use invoke_util::{decode_install_request, mpk_package_name};
pub(crate) use invoke_util::{
    find_app, legacy_invoke_async_proc, legacy_invoke_sync_proc, parse_json_object_body,
    runtime_get_app_and_desc, to_param,
};
#[cfg(test)]
pub(crate) use serve::configure_routes;
pub use serve::{serve_http_api, serve_http_api_on_listener, serve_http_api_on_listener_with_stop};

// These tests spin up an actix-web app. actix-router intentionally leaks route
// capture names via `Box::leak`, which Miri reports as reachable memory leaks.
// Each test below returns early when running under Miri.
#[cfg(test)]
mod tests;

use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::json::JsonValue;
use mudu_contract::procedure::proc_desc::ProcDesc;
use serde_json::Value;

/// Management HTTP API surface exposed by a backend; the route handlers call
/// into this trait.
#[async_trait(?Send)]
pub trait HttpApi: Send + Sync {
    /// Lists the names of installed applications.
    async fn list_apps(&self) -> RS<Vec<String>>;
    /// Lists the procedures of an application as `mod_name/proc_name` pairs.
    async fn list_procedures(&self, app_name: &str) -> RS<Vec<String>>;
    /// Returns the procedure descriptor and the default parameter/return
    /// JSON values.
    async fn procedure_detail(
        &self,
        app_name: &str,
        mod_name: &str,
        proc_name: &str,
    ) -> RS<(ProcDesc, JsonValue, JsonValue)>;
    /// Installs an application package (mpk) binary.
    async fn install_mpk(&self, mpk_binary: Vec<u8>) -> RS<()>;
    /// Invokes a procedure with a JSON object body and returns the result as
    /// JSON.
    async fn invoke_json(
        &self,
        app_name: &str,
        mod_name: &str,
        proc_name: &str,
        body: String,
    ) -> RS<Value>;

    /// Routes a point key or key range of a partition rule to the partitions
    /// (and their workers) covering it.
    async fn route_partition(&self, _request: PartitionRouteRequest) -> RS<PartitionRouteResponse> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "partition route is not supported"
        ))
    }

    /// Returns the worker/replication topology view of this server.
    async fn server_topology(&self) -> RS<ServerTopology> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "server topology is not supported"
        ))
    }

    /// Returns the replication status of this server.
    async fn server_replication(&self) -> RS<ServerReplicationResponse> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "server replication status is not supported"
        ))
    }

    /// Uninstalls an application by name.
    async fn uninstall_app(&self, app_name: &str) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!("uninstall is not supported for {}", app_name)
        ))
    }

    /// Applies a raft membership change (add/remove voter or learner,
    /// promote learner).
    async fn repl_membership(
        &self,
        _request: ReplMembershipRequest,
    ) -> Result<ReplMembershipResponse, ::repl::raft_service::ReplMembershipError> {
        Err(::repl::raft_service::ReplMembershipError {
            error: mudu_error!(
                ErrorCode::NotImplemented,
                "replication membership is not supported"
            ),
            leader_hint: None,
        })
    }

    /// Promotes this node to stream primary.
    async fn repl_promote(
        &self,
        _request: ReplPromoteRequest,
    ) -> Result<ReplRoleResponse, ::repl::stream_admin::StreamAdminError> {
        Err(::repl::stream_admin::StreamAdminError {
            error: mudu_error!(ErrorCode::NotImplemented, "stream promote is not supported"),
            bad_request: false,
        })
    }

    /// Makes this node follow the given stream primary.
    async fn repl_follow(
        &self,
        _request: ReplFollowRequest,
    ) -> Result<ReplRoleResponse, ::repl::stream_admin::StreamAdminError> {
        Err(::repl::stream_admin::StreamAdminError {
            error: mudu_error!(ErrorCode::NotImplemented, "stream follow is not supported"),
            bad_request: false,
        })
    }

    /// Takes a base backup under the given label.
    async fn backup(&self, _label: &str) -> RS<Value> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "base backup is not supported"
        ))
    }

    /// Lists the labels of existing base backups.
    async fn backup_list(&self) -> RS<Vec<String>> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "base backup is not supported"
        ))
    }

    /// Verifies the integrity of a base backup.
    async fn backup_verify(&self, _label: &str) -> RS<Value> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "base backup is not supported"
        ))
    }

    /// Deletes a base backup, returning whether it existed.
    async fn backup_delete(&self, _label: &str) -> RS<bool> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "base backup is not supported"
        ))
    }

    /// Writes a logical dump of the database into `dir` under the given
    /// label.
    async fn dump_logical(&self, _label: &str, _dir: &str) -> RS<Value> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "logical dump is not supported"
        ))
    }

    /// Restores a logical dump from `dir`, optionally dropping existing
    /// objects first.
    async fn restore_logical(&self, _label: &str, _dir: &str, _drop_existing: bool) -> RS<Value> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "logical restore is not supported"
        ))
    }
}

/// Client channel to the kernel's procedure-invoke TCP endpoint.
#[async_trait(?Send)]
pub trait AsyncKernelInvokeClient: Send {
    /// Opens a session and returns its id.
    async fn create_session(&mut self, config_json: Option<String>) -> RS<u128>;
    /// Invokes a procedure with serialized parameters, returning the
    /// serialized result.
    async fn invoke_procedure(
        &mut self,
        session_id: u128,
        procedure_name: String,
        procedure_parameters: Vec<u8>,
    ) -> RS<Vec<u8>>;
    /// Closes a session.
    async fn close_session(&mut self, session_id: u128) -> RS<bool>;
}

/// Factory connecting [`AsyncKernelInvokeClient`] instances.
#[async_trait(?Send)]
pub trait AsyncKernelInvokeClientFactory: Send + Sync {
    /// Connects to the kernel invoke endpoint at `addr`.
    async fn connect(&self, addr: &str) -> RS<Box<dyn AsyncKernelInvokeClient>>;
}
