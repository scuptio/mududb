use super::*;
use ::conn::worker_local::{
    set_current_worker_local, try_current_worker_local, unset_current_worker_local, WorkerLocalRef,
};
use mudu_contract::protocol::{ProcedureInvokeRequest, ProcedureInvokeResponse};
use mudu_utils::task_trace;

impl WorkerRuntime {
    pub(crate) async fn invoke_procedure(
        &self,
        session_id: OID,
        procedure_name: &str,
        procedure_parameters: Vec<u8>,
        worker_local: WorkerLocalRef,
    ) -> RS<Vec<u8>> {
        let trace = task_trace!();
        trace.watch(
            "procedure.kernel.worker_invoke.stage",
            "runtime_lookup_start",
        );
        trace.watch(
            "procedure.kernel.worker_invoke.session_id",
            &session_id.to_string(),
        );
        trace.watch("procedure.kernel.worker_invoke.name", procedure_name);
        let procedure_runtime = self.procedure_runtime.as_ref().ok_or_else(|| {
            mudu_error!(
                ErrorCode::NotImplemented,
                "procedure runtime is not configured"
            )
        })?;
        trace.watch(
            "procedure.kernel.worker_invoke.stage",
            "runtime_lookup_done",
        );
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::ProcInvoke);
        let result = procedure_runtime
            .invoke(
                session_id,
                procedure_name,
                procedure_parameters,
                worker_local,
            )
            .await;
        trace.watch(
            "procedure.kernel.worker_invoke.stage",
            if result.is_ok() {
                "invoke_done"
            } else {
                "invoke_error"
            },
        );
        result
    }

    /// Handle one procedure-invoke request arriving on connection `conn_id`:
    /// verify the session belongs to the connection, install a session-bound
    /// worker-local context, invoke the configured procedure runtime, and
    /// wrap the serialized result in the response.
    pub async fn handle_procedure_request(
        &self,
        conn_id: u64,
        request: &ProcedureInvokeRequest,
    ) -> RS<ProcedureInvokeResponse> {
        let trace = task_trace!();
        trace.watch("procedure.kernel.handle.stage", "enter");
        trace.watch("procedure.kernel.handle.conn_id", &conn_id.to_string());
        let session_id = request.session_id() as OID;
        trace.watch(
            "procedure.kernel.handle.session_id",
            &session_id.to_string(),
        );
        trace.watch("procedure.kernel.handle.name", request.procedure_name());
        trace.watch("procedure.kernel.handle.stage", "ensure_session_owner");
        self.ensure_session_owned_by_connection(conn_id, session_id)?;
        trace.watch("procedure.kernel.handle.stage", "worker_local_create");
        let worker_local =
            as_worker_local_ref(new_session_bound_worker_runtime(self.clone(), session_id));
        let prev_worker_local = try_current_worker_local();
        trace.watch("procedure.kernel.handle.stage", "worker_local_set");
        set_current_worker_local(worker_local.clone());
        trace.watch("procedure.kernel.handle.stage", "invoke_start");
        let result = self
            .invoke_procedure(
                session_id,
                request.procedure_name(),
                request.procedure_parameters_owned(),
                worker_local,
            )
            .await;
        trace.watch(
            "procedure.kernel.handle.stage",
            if result.is_ok() {
                "invoke_done"
            } else {
                "invoke_error"
            },
        );
        if let Some(prev_worker_local) = prev_worker_local {
            trace.watch("procedure.kernel.handle.stage", "restore_prev_worker_local");
            set_current_worker_local(prev_worker_local);
        } else {
            trace.watch("procedure.kernel.handle.stage", "unset_worker_local");
            unset_current_worker_local();
        }
        trace.watch("procedure.kernel.handle.stage", "response_build");
        Ok(ProcedureInvokeResponse::new(result?))
    }
}
