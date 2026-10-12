use super::HttpApiContext;
use super::dto::{
    PartitionRouteRequest, ReplFollowRequest, ReplMembershipRequest, ReplPromoteRequest,
};
use super::invoke_util::{decode_install_request, mpk_package_name};
use super::procedure_list::ProcedureList;
use actix_web::{HttpResponse, Responder, delete, get, post, web};
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use mudu::utils::json::JsonValue;
use mudu_utils::scoped_task_trace;
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn http_ok(data: Value) -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "data": data,
        "error": Value::Null,
        "status": 0,
        "message": "ok"
    }))
}

fn error_payload(err: &MuduError) -> Value {
    serde_json::json!({
        "code": err.ec().to_u32(),
        "name": format!("{:?}", err.ec()),
        "message": err.message(),
        "source": err.err_src().to_json_str(),
        "location": err.loc()
    })
}

fn http_err(user_message: impl Into<String>, err: &MuduError) -> HttpResponse {
    let msg = user_message.into();
    let payload = error_payload(err);
    HttpResponse::Ok().json(serde_json::json!({
        "ok": false,
        "data": payload.clone(),
        "error": payload.clone(),
        "status": err.ec().to_u32(),
        "message": msg,
    }))
}

#[post("/mudu/partition/route")]
pub(super) async fn partition_route(
    body: String,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    let request = match serde_json::from_str::<PartitionRouteRequest>(&body) {
        Ok(request) => request,
        Err(e) => {
            let err = mudu_error!(
                ErrorCode::Decode,
                "fail to parse partition route request",
                e
            );
            return http_err("fail to parse partition route request", &err);
        }
    };
    match context.api.route_partition(request).await {
        Ok(route) => http_ok(serde_json::to_value(route).unwrap_or(Value::Null)),
        Err(e) => http_err("fail to route partition", &e),
    }
}

#[get("/mudu/server/topology")]
pub(super) async fn server_topology(context: web::Data<HttpApiContext>) -> impl Responder {
    match context.api.server_topology().await {
        Ok(topology) => http_ok(serde_json::to_value(topology).unwrap_or(Value::Null)),
        Err(e) => http_err("fail to get server topology", &e),
    }
}

#[get("/mudu/server/replication")]
pub(super) async fn server_replication(context: web::Data<HttpApiContext>) -> impl Responder {
    match context.api.server_replication().await {
        Ok(status) => http_ok(serde_json::to_value(status).unwrap_or(Value::Null)),
        Err(e) => http_err("fail to get server replication status", &e),
    }
}

#[post("/mudu/repl/membership")]
pub(super) async fn repl_membership(
    body: String,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    let request = match serde_json::from_str::<ReplMembershipRequest>(&body) {
        Ok(request) => request,
        Err(e) => {
            let err = mudu_error!(
                ErrorCode::Decode,
                "fail to parse replication membership request",
                e
            );
            return http_err("fail to parse replication membership request", &err);
        }
    };
    match context.api.repl_membership(request).await {
        Ok(view) => http_ok(serde_json::to_value(view).unwrap_or(Value::Null)),
        Err(e) => {
            let mut payload = error_payload(&e.error);
            if let Some(hint) = &e.leader_hint {
                payload["leader_hint"] = serde_json::json!({
                    "node_id": hint.node_id,
                    "host": hint.host,
                    "http_port": hint.http_port,
                });
            }
            HttpResponse::Ok().json(serde_json::json!({
                "ok": false,
                "data": payload.clone(),
                "error": payload.clone(),
                "status": e.error.ec().to_u32(),
                "message": "fail to update replication membership",
            }))
        }
    }
}

/// Renders a stream admin error: caller errors (e.g. a missing fencing
/// confirmation) come back as HTTP 400, everything else as the usual
/// ok:false envelope.
fn http_stream_admin_err(
    user_message: &str,
    err: &::repl::stream_admin::StreamAdminError,
) -> HttpResponse {
    let payload = error_payload(&err.error);
    let body = serde_json::json!({
        "ok": false,
        "data": payload.clone(),
        "error": payload.clone(),
        "status": err.error.ec().to_u32(),
        "message": user_message,
    });
    if err.bad_request {
        HttpResponse::BadRequest().json(body)
    } else {
        HttpResponse::Ok().json(body)
    }
}

#[post("/mudu/repl/promote")]
pub(super) async fn repl_promote(
    body: String,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    let request = match serde_json::from_str::<ReplPromoteRequest>(&body) {
        Ok(request) => request,
        Err(e) => {
            let err = mudu_error!(ErrorCode::Decode, "fail to parse stream promote request", e);
            return http_err("fail to parse stream promote request", &err);
        }
    };
    match context.api.repl_promote(request).await {
        Ok(view) => http_ok(serde_json::to_value(view).unwrap_or(Value::Null)),
        Err(e) => http_stream_admin_err("fail to promote this node", &e),
    }
}

#[post("/mudu/repl/follow")]
pub(super) async fn repl_follow(
    body: String,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    let request = match serde_json::from_str::<ReplFollowRequest>(&body) {
        Ok(request) => request,
        Err(e) => {
            let err = mudu_error!(ErrorCode::Decode, "fail to parse stream follow request", e);
            return http_err("fail to parse stream follow request", &err);
        }
    };
    match context.api.repl_follow(request).await {
        Ok(view) => http_ok(serde_json::to_value(view).unwrap_or(Value::Null)),
        Err(e) => http_stream_admin_err("fail to follow the requested primary", &e),
    }
}

/// Request body of the backup administration endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupRequest {
    /// Backup label to create, verify, or delete.
    label: String,
}

#[post("/mudu/backup")]
pub(super) async fn backup(body: String, context: web::Data<HttpApiContext>) -> impl Responder {
    let request = match serde_json::from_str::<BackupRequest>(&body) {
        Ok(request) => request,
        Err(e) => {
            let err = mudu_error!(ErrorCode::Decode, "fail to parse backup request", e);
            return http_err("fail to parse backup request", &err);
        }
    };
    match context.api.backup(&request.label).await {
        Ok(label) => http_ok(label),
        Err(e) => http_err("fail to take a base backup", &e),
    }
}

#[get("/mudu/backup/list")]
pub(super) async fn backup_list(context: web::Data<HttpApiContext>) -> impl Responder {
    match context.api.backup_list().await {
        Ok(list) => http_ok(serde_json::to_value(list).unwrap_or(Value::Null)),
        Err(e) => http_err("fail to list backups", &e),
    }
}

#[post("/mudu/backup/verify")]
pub(super) async fn backup_verify(
    body: String,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    let request = match serde_json::from_str::<BackupRequest>(&body) {
        Ok(request) => request,
        Err(e) => {
            let err = mudu_error!(ErrorCode::Decode, "fail to parse backup verify request", e);
            return http_err("fail to parse backup verify request", &err);
        }
    };
    match context.api.backup_verify(&request.label).await {
        Ok(label) => http_ok(label),
        Err(e) => http_err("fail to verify backup", &e),
    }
}

#[post("/mudu/backup/delete")]
pub(super) async fn backup_delete(
    body: String,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    let request = match serde_json::from_str::<BackupRequest>(&body) {
        Ok(request) => request,
        Err(e) => {
            let err = mudu_error!(ErrorCode::Decode, "fail to parse backup delete request", e);
            return http_err("fail to parse backup delete request", &err);
        }
    };
    match context.api.backup_delete(&request.label).await {
        Ok(deleted) => http_ok(serde_json::to_value(deleted).unwrap_or(Value::Null)),
        Err(e) => http_err("fail to delete backup", &e),
    }
}

/// Request body of the logical dump endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogicalDumpRequest {
    label: String,
    dir: String,
}

/// Request body of the logical restore endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogicalRestoreRequest {
    label: String,
    dir: String,
    #[serde(default)]
    drop_existing: bool,
}

#[post("/mudu/dump")]
pub(super) async fn dump_logical(
    body: String,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    let request = match serde_json::from_str::<LogicalDumpRequest>(&body) {
        Ok(request) => request,
        Err(e) => {
            let err = mudu_error!(ErrorCode::Decode, "fail to parse dump request", e);
            return http_err("fail to parse dump request", &err);
        }
    };
    match context.api.dump_logical(&request.label, &request.dir).await {
        Ok(report) => http_ok(report),
        Err(e) => http_err("fail to take a logical dump", &e),
    }
}

#[post("/mudu/restore-logical")]
pub(super) async fn restore_logical(
    body: String,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    let request = match serde_json::from_str::<LogicalRestoreRequest>(&body) {
        Ok(request) => request,
        Err(e) => {
            let err = mudu_error!(ErrorCode::Decode, "fail to parse restore request", e);
            return http_err("fail to parse restore request", &err);
        }
    };
    match context
        .api
        .restore_logical(&request.label, &request.dir, request.drop_existing)
        .await
    {
        Ok(report) => http_ok(report),
        Err(e) => http_err("fail to restore the logical dump", &e),
    }
}

#[get("/mudu/app/list")]
pub(super) async fn app_list(context: web::Data<HttpApiContext>) -> impl Responder {
    match context.api.list_apps().await {
        Ok(list) => http_ok(serde_json::to_value(list).unwrap_or(Value::Null)),
        Err(e) => http_err("fail to get app list", &e),
    }
}

#[get("/mudu/app/list/{app_name}")]
pub(super) async fn app_proc_list(
    path: web::Path<String>,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    scoped_task_trace!();
    let app_name = path.into_inner();
    match context.api.list_procedures(&app_name).await {
        Ok(procedures) => http_ok(
            serde_json::to_value(ProcedureList {
                app_name,
                procedures,
            })
            .unwrap_or(Value::Null),
        ),
        Err(e) => http_err(
            format!("fail to get procedure list of app {}", app_name),
            &e,
        ),
    }
}

#[get("/mudu/app/list/{app_name}/{mod_name}/{proc_name}")]
pub(super) async fn app_proc_detail(
    path: web::Path<(String, String, String)>,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    scoped_task_trace!();
    let (app_name, mod_name, proc_name) = path.into_inner();
    match context
        .api
        .procedure_detail(&app_name, &mod_name, &proc_name)
        .await
    {
        Ok((desc, param_json_default, return_json_default)) => http_ok(serde_json::json!({
            "proc_desc": desc,
            "param_default": param_json_default,
            "return_default": return_json_default,
        })),
        Err(e) => http_err(
            format!(
                "fail to get procedure {}/{}/{} detail ",
                app_name, mod_name, proc_name
            ),
            &e,
        ),
    }
}

#[post("/mudu/app/install")]
pub(super) async fn install(
    body: web::Bytes,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    let body_str = String::from_utf8_lossy(&body).to_string();
    match decode_install_request(&body_str) {
        Ok(binary) => {
            let package_name = mpk_package_name(&binary).unwrap_or_else(|| "<unknown>".to_string());
            match context.api.install_mpk(binary).await {
                Ok(()) => http_ok(JsonValue::Null),
                Err(e) => http_err(format!("fail to install package {}", package_name), &e),
            }
        }
        Err(e) => http_err("fail to install package <invalid request>", &e),
    }
}

#[delete("/mudu/app/uninstall/{app_name}")]
pub(super) async fn uninstall(
    path: web::Path<String>,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    let app_name = path.into_inner();
    match context.api.uninstall_app(&app_name).await {
        Ok(()) => http_ok(JsonValue::Null),
        Err(e) => http_err(format!("fail to uninstall app {}", app_name), &e),
    }
}

#[post("/mudu/app/invoke/{app_name}/{mod_name}/{proc_name}")]
pub(super) async fn invoke(
    path: web::Path<(String, String, String)>,
    body: web::Bytes,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    let (app_name, mod_name, proc_name) = path.into_inner();
    let body_str = String::from_utf8_lossy(&body).to_string();
    let proc = format!("{}/{}/{}", app_name, mod_name, proc_name);
    match context
        .api
        .invoke_json(&app_name, &mod_name, &proc_name, body_str)
        .await
    {
        Ok(value) => http_ok(value),
        Err(e) => http_err(format!("fail to invoke procedure {}", proc), &e),
    }
}
