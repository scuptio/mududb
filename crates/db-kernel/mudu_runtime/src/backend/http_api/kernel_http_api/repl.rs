use super::super::{
    ReplFollowRequest, ReplMembershipMemberView, ReplMembershipRequest, ReplMembershipResponse,
    ReplPromoteRequest, ReplRoleResponse,
};
use super::KernelHttpApi;
use mudu::error::ErrorCode;
use mudu::mudu_error;

/// Replication-admin endpoints (raft membership and stream primary/follow).
impl KernelHttpApi {
    pub(super) async fn repl_membership(
        &self,
        request: ReplMembershipRequest,
    ) -> Result<ReplMembershipResponse, ::repl::raft_service::ReplMembershipError> {
        use ::repl::raft_service::{ReplMembershipAction, raft_membership_update};
        let action = match request.action.as_str() {
            "add_voter" => ReplMembershipAction::AddVoter,
            "add_learner" => ReplMembershipAction::AddLearner,
            "remove" => ReplMembershipAction::Remove,
            "promote_learner" => ReplMembershipAction::PromoteLearner,
            other => {
                return Err(::repl::raft_service::ReplMembershipError {
                    error: mudu_error!(
                        ErrorCode::InvalidArgument,
                        format!(
                            "unknown membership action \"{other}\": expected add_voter | add_learner | remove | promote_learner"
                        )
                    ),
                    leader_hint: None,
                });
            }
        };
        let Some(node_id) = self.raft_node_id else {
            return Err(::repl::raft_service::ReplMembershipError {
                error: mudu_error!(
                    ErrorCode::InvalidState,
                    "replication membership requires [replication.raft] mode; this server runs no replication"
                ),
                leader_hint: None,
            });
        };
        let http_port = self.http_listen_port;
        // The raft call blocks until the joint commit completes; bridge it
        // off the actix runtime (heal-barrier-wait pattern).
        let (tx, rx) = mudu_sys::tokio::sync::oneshot::channel();
        mudu_sys::task::sync::spawn_thread_named("repl-membership".to_string(), move || {
            let result = raft_membership_update(
                node_id,
                action,
                request.node_id,
                request.host,
                request.port,
                http_port,
            );
            let _ = tx.send(result);
            Ok::<_, mudu::error::MuduError>(())
        })
        .map_err(|error| ::repl::raft_service::ReplMembershipError {
            error,
            leader_hint: None,
        })?;
        let view = rx
            .await
            .map_err(|e| ::repl::raft_service::ReplMembershipError {
                error: mudu_error!(
                    ErrorCode::ChannelClosed,
                    "membership worker dropped the request",
                    e
                ),
                leader_hint: None,
            })??;
        Ok(ReplMembershipResponse {
            members: view
                .members
                .into_iter()
                .map(|member| ReplMembershipMemberView {
                    node_id: member.node_id,
                    can_vote: member.can_vote,
                    host: member.host,
                    port: member.port,
                })
                .collect(),
            conf_term: view.conf_term,
            conf_version: view.conf_version,
        })
    }

    pub(super) async fn repl_follow(
        &self,
        request: ReplFollowRequest,
    ) -> Result<ReplRoleResponse, ::repl::stream_admin::StreamAdminError> {
        let Some(node_id) = self.raft_node_id else {
            return Err(::repl::stream_admin::StreamAdminError {
                error: mudu_error!(
                    ErrorCode::InvalidState,
                    "stream follow requires [replication.stream] mode; this server runs no replication"
                ),
                bad_request: false,
            });
        };
        let view = ::repl::stream_admin::stream_follow(
            node_id,
            request.primary_node_id,
            request.primary_host,
            request.primary_port,
        )
        .await?;
        Ok(ReplRoleResponse {
            role: format!("{:?}", view.role).to_lowercase(),
            epoch: view.epoch,
            primary_node_id: view.primary_node_id,
        })
    }

    pub(super) async fn repl_promote(
        &self,
        request: ReplPromoteRequest,
    ) -> Result<ReplRoleResponse, ::repl::stream_admin::StreamAdminError> {
        let Some(node_id) = self.raft_node_id else {
            return Err(::repl::stream_admin::StreamAdminError {
                error: mudu_error!(
                    ErrorCode::InvalidState,
                    "stream promote requires [replication.stream] mode; this server runs no replication"
                ),
                bad_request: false,
            });
        };
        let fencing_ok = request.fencing_ok;
        let (tx, rx) = mudu_sys::tokio::sync::oneshot::channel();
        mudu_sys::task::sync::spawn_thread_named("repl-promote".to_string(), move || {
            let result = ::repl::stream_admin::stream_promote(node_id, fencing_ok);
            let _ = tx.send(result);
            Ok::<_, mudu::error::MuduError>(())
        })
        .map_err(|error| ::repl::stream_admin::StreamAdminError {
            error,
            bad_request: false,
        })?;
        let view = rx
            .await
            .map_err(|e| ::repl::stream_admin::StreamAdminError {
                error: mudu_error!(
                    ErrorCode::ChannelClosed,
                    "promote worker dropped the request",
                    e
                ),
                bad_request: false,
            })??;
        Ok(ReplRoleResponse {
            role: format!("{:?}", view.role).to_lowercase(),
            epoch: view.epoch,
            primary_node_id: view.primary_node_id,
        })
    }
}
