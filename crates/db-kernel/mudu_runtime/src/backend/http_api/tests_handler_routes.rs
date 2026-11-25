//! Handler-level route tests: JSON parse failures, error envelopes, and the
//! ok paths of the backup / logical-dump / partition-route handlers.

use super::*;
use actix_web::{App, http::StatusCode, test, web};
use mudu_contract::procedure::proc_desc::ProcDesc;
use serde_json::Value;
use std::sync::Arc;
// ---------------------------------------------------------------------------
// Handler-level route tests: JSON parse failures, error envelopes, and the
// ok paths of the backup / logical-dump / partition-route handlers.
// ---------------------------------------------------------------------------

/// Serves ok responses for the backup, logical dump/restore, and partition
/// route endpoints.
struct BackupHttpApi;

#[async_trait(?Send)]
impl HttpApi for BackupHttpApi {
    async fn list_apps(&self) -> RS<Vec<String>> {
        Ok(vec![])
    }

    async fn list_procedures(&self, _app_name: &str) -> RS<Vec<String>> {
        Ok(vec![])
    }

    async fn procedure_detail(
        &self,
        _app_name: &str,
        _mod_name: &str,
        _proc_name: &str,
    ) -> RS<(ProcDesc, JsonValue, JsonValue)> {
        Err(mudu_error!(ErrorCode::ObjectNotFound, "no such procedure"))
    }

    async fn install_mpk(&self, _mpk_binary: Vec<u8>) -> RS<()> {
        Ok(())
    }

    async fn invoke_json(
        &self,
        _app_name: &str,
        _mod_name: &str,
        _proc_name: &str,
        _body: String,
    ) -> RS<Value> {
        Ok(Value::Null)
    }

    async fn route_partition(&self, _request: PartitionRouteRequest) -> RS<PartitionRouteResponse> {
        Ok(PartitionRouteResponse {
            routes: vec![],
            rule_version: 1,
            topology_version: 0,
            placement_version: 0,
        })
    }

    async fn backup(&self, label: &str) -> RS<Value> {
        Ok(serde_json::json!({"label": label}))
    }

    async fn backup_list(&self) -> RS<Vec<String>> {
        Ok(vec!["base1".to_string(), "base2".to_string()])
    }

    async fn backup_verify(&self, label: &str) -> RS<Value> {
        Ok(serde_json::json!({"label": label, "ok": true}))
    }

    async fn backup_delete(&self, _label: &str) -> RS<bool> {
        Ok(true)
    }

    async fn dump_logical(&self, label: &str, dir: &str) -> RS<Value> {
        Ok(serde_json::json!({"label": label, "dir": dir}))
    }

    async fn restore_logical(&self, label: &str, dir: &str, drop_existing: bool) -> RS<Value> {
        Ok(serde_json::json!({"label": label, "dir": dir, "drop_existing": drop_existing}))
    }
}

/// Fails every endpoint; the membership failure carries a leader hint and the
/// promote failure is marked as a caller (bad-request) error.
struct FailingHttpApi;

#[async_trait(?Send)]
impl HttpApi for FailingHttpApi {
    async fn list_apps(&self) -> RS<Vec<String>> {
        Err(mudu_error!(ErrorCode::ObjectNotFound, "mock failure"))
    }

    async fn list_procedures(&self, _app_name: &str) -> RS<Vec<String>> {
        Err(mudu_error!(ErrorCode::ObjectNotFound, "mock failure"))
    }

    async fn procedure_detail(
        &self,
        _app_name: &str,
        _mod_name: &str,
        _proc_name: &str,
    ) -> RS<(ProcDesc, JsonValue, JsonValue)> {
        Err(mudu_error!(ErrorCode::ObjectNotFound, "mock failure"))
    }

    async fn install_mpk(&self, _mpk_binary: Vec<u8>) -> RS<()> {
        Err(mudu_error!(ErrorCode::ObjectNotFound, "mock failure"))
    }

    async fn invoke_json(
        &self,
        _app_name: &str,
        _mod_name: &str,
        _proc_name: &str,
        _body: String,
    ) -> RS<Value> {
        Err(mudu_error!(ErrorCode::ObjectNotFound, "mock failure"))
    }

    async fn repl_membership(
        &self,
        _request: ReplMembershipRequest,
    ) -> Result<ReplMembershipResponse, ::repl::raft_service::ReplMembershipError> {
        Err(::repl::raft_service::ReplMembershipError {
            error: mudu_error!(ErrorCode::InvalidState, "not the leader"),
            leader_hint: Some(::repl::raft_service::ReplLeaderHttpHint {
                node_id: 7,
                host: "127.0.0.1".to_string(),
                http_port: 8300,
            }),
        })
    }

    async fn repl_promote(
        &self,
        _request: ReplPromoteRequest,
    ) -> Result<ReplRoleResponse, ::repl::stream_admin::StreamAdminError> {
        Err(::repl::stream_admin::StreamAdminError {
            error: mudu_error!(ErrorCode::InvalidArgument, "fencing not confirmed"),
            bad_request: true,
        })
    }
}

#[actix_web::test]
async fn json_body_routes_reject_invalid_json() {
    if cfg!(miri) {
        // actix-router leaks route capture names via Box::leak; skip under Miri.
        return;
    }
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(HttpApiContext {
                api: Arc::new(BackupHttpApi),
            }))
            .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::IOURING)),
    )
    .await;

    for uri in [
        "/mudu/partition/route",
        "/mudu/server/queue_activation",
        "/mudu/repl/membership",
        "/mudu/repl/promote",
        "/mudu/repl/follow",
        "/mudu/backup",
        "/mudu/backup/verify",
        "/mudu/backup/delete",
        "/mudu/dump",
        "/mudu/restore-logical",
    ] {
        let request = test::TestRequest::post()
            .uri(uri)
            .set_payload("{not valid json")
            .to_request();
        let response: Value = test::call_and_read_body_json(&app, request).await;
        assert_eq!(response["ok"], false, "uri {uri}");
        assert_eq!(response["status"], ErrorCode::Decode.to_u32(), "uri {uri}");
    }
}

#[actix_web::test]
async fn backup_and_logical_dump_routes_return_ok_envelopes() {
    if cfg!(miri) {
        return;
    }
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(HttpApiContext {
                api: Arc::new(BackupHttpApi),
            }))
            .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::IOURING)),
    )
    .await;

    let request = test::TestRequest::post()
        .uri("/mudu/backup")
        .set_payload(r#"{"label":"b1"}"#)
        .to_request();
    let response: Value = test::call_and_read_body_json(&app, request).await;
    assert_eq!(response["status"], 0);
    assert_eq!(response["data"]["label"], "b1");

    let request = test::TestRequest::get()
        .uri("/mudu/backup/list")
        .to_request();
    let response: Value = test::call_and_read_body_json(&app, request).await;
    assert_eq!(response["status"], 0);
    assert_eq!(response["data"][0], "base1");
    assert_eq!(response["data"][1], "base2");

    let request = test::TestRequest::post()
        .uri("/mudu/backup/verify")
        .set_payload(r#"{"label":"b1"}"#)
        .to_request();
    let response: Value = test::call_and_read_body_json(&app, request).await;
    assert_eq!(response["status"], 0);
    assert_eq!(response["data"]["ok"], true);

    let request = test::TestRequest::post()
        .uri("/mudu/backup/delete")
        .set_payload(r#"{"label":"b1"}"#)
        .to_request();
    let response: Value = test::call_and_read_body_json(&app, request).await;
    assert_eq!(response["status"], 0);
    assert_eq!(response["data"], true);

    let request = test::TestRequest::post()
        .uri("/mudu/dump")
        .set_payload(r#"{"label":"b1","dir":"/tmp/dump"}"#)
        .to_request();
    let response: Value = test::call_and_read_body_json(&app, request).await;
    assert_eq!(response["status"], 0);
    assert_eq!(response["data"]["dir"], "/tmp/dump");

    let request = test::TestRequest::post()
        .uri("/mudu/restore-logical")
        .set_payload(r#"{"label":"b1","dir":"/tmp/dump","drop_existing":true}"#)
        .to_request();
    let response: Value = test::call_and_read_body_json(&app, request).await;
    assert_eq!(response["status"], 0);
    assert_eq!(response["data"]["drop_existing"], true);

    // The partition-route ok path serializes the route list.
    let request = test::TestRequest::post()
        .uri("/mudu/partition/route")
        .set_payload(r#"{"rule_name":"r","key":["1"]}"#)
        .to_request();
    let response: Value = test::call_and_read_body_json(&app, request).await;
    assert_eq!(response["status"], 0);
    assert_eq!(response["data"]["routes"], serde_json::json!([]));
}

#[actix_web::test]
async fn failing_backend_surfaces_error_envelopes() {
    if cfg!(miri) {
        return;
    }
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(HttpApiContext {
                api: Arc::new(FailingHttpApi),
            }))
            .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::IOURING)),
    )
    .await;

    // Endpoints backed by FailingHttpApi overrides report the mock's code.
    for uri in [
        "/mudu/app/list",
        "/mudu/app/list/app1",
        "/mudu/app/list/app1/mod1/proc1",
    ] {
        let request = test::TestRequest::get().uri(uri).to_request();
        let response: Value = test::call_and_read_body_json(&app, request).await;
        assert_eq!(response["ok"], false, "uri {uri}");
        assert_eq!(
            response["status"],
            ErrorCode::ObjectNotFound.to_u32(),
            "uri {uri}"
        );
    }

    // Endpoints backed by the trait's default bodies report NotImplemented.
    for uri in [
        "/mudu/server/replication",
        "/mudu/server/queues",
        "/mudu/server/queue_activations",
        "/mudu/backup/list",
    ] {
        let request = test::TestRequest::get().uri(uri).to_request();
        let response: Value = test::call_and_read_body_json(&app, request).await;
        assert_eq!(response["ok"], false, "uri {uri}");
        assert_eq!(
            response["status"],
            ErrorCode::NotImplemented.to_u32(),
            "uri {uri}"
        );
    }

    let request = test::TestRequest::delete()
        .uri("/mudu/app/uninstall/app1")
        .to_request();
    let response: Value = test::call_and_read_body_json(&app, request).await;
    assert_eq!(response["ok"], false);

    for (uri, body) in [
        ("/mudu/partition/route", r#"{"rule_name":"r","key":["1"]}"#),
        ("/mudu/backup", r#"{"label":"b1"}"#),
        ("/mudu/backup/verify", r#"{"label":"b1"}"#),
        ("/mudu/backup/delete", r#"{"label":"b1"}"#),
        ("/mudu/dump", r#"{"label":"b1","dir":"/tmp/d"}"#),
        ("/mudu/restore-logical", r#"{"label":"b1","dir":"/tmp/d"}"#),
    ] {
        let request = test::TestRequest::post()
            .uri(uri)
            .set_payload(body)
            .to_request();
        let response: Value = test::call_and_read_body_json(&app, request).await;
        assert_eq!(response["ok"], false, "uri {uri}");
        assert_eq!(
            response["status"],
            ErrorCode::NotImplemented.to_u32(),
            "uri {uri}"
        );
    }
}

#[actix_web::test]
async fn repl_membership_error_carries_leader_hint() {
    if cfg!(miri) {
        return;
    }
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(HttpApiContext {
                api: Arc::new(FailingHttpApi),
            }))
            .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::IOURING)),
    )
    .await;

    let request = test::TestRequest::post()
        .uri("/mudu/repl/membership")
        .set_payload(r#"{"action":"remove","node_id":3}"#)
        .to_request();
    let response: Value = test::call_and_read_body_json(&app, request).await;
    assert_eq!(response["ok"], false);
    assert_eq!(response["status"], ErrorCode::InvalidState.to_u32());
    let hint = &response["data"]["leader_hint"];
    assert_eq!(hint["node_id"], 7);
    assert_eq!(hint["host"], "127.0.0.1");
    assert_eq!(hint["http_port"], 8300);
}

#[actix_web::test]
async fn repl_promote_bad_request_maps_to_http_400() {
    if cfg!(miri) {
        return;
    }
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(HttpApiContext {
                api: Arc::new(FailingHttpApi),
            }))
            .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::IOURING)),
    )
    .await;

    let request = test::TestRequest::post()
        .uri("/mudu/repl/promote")
        .set_payload(r#"{"fencing_ok":false}"#)
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["ok"], false);
    assert_eq!(body["status"], ErrorCode::InvalidArgument.to_u32());
}

#[actix_web::test]
async fn install_route_accepts_valid_body_and_rejects_shape_errors() {
    if cfg!(miri) {
        return;
    }
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(HttpApiContext {
                api: Arc::new(BackupHttpApi),
            }))
            .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::LEGACY)),
    )
    .await;

    // base64("mpk") — the mock install accepts any bytes.
    let request = test::TestRequest::post()
        .uri("/mudu/app/install")
        .set_payload(r#"{"mpk_base64":"bXBr"}"#)
        .to_request();
    let response: Value = test::call_and_read_body_json(&app, request).await;
    assert_eq!(response["status"], 0);

    let request = test::TestRequest::post()
        .uri("/mudu/app/install")
        .set_payload(r#"{"wrong_field":"bXBr"}"#)
        .to_request();
    let response: Value = test::call_and_read_body_json(&app, request).await;
    assert_eq!(response["ok"], false);
    assert_eq!(response["status"], ErrorCode::InvalidArgument.to_u32());
}
