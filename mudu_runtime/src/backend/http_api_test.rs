#[cfg(test)]
mod tests {
    use super::super::http_api::*;
    use actix_web::{App, http::StatusCode, test as actix_test, web};
    use async_trait::async_trait;
    use base64::Engine;
    use mudu::common::app_info::AppInfo;
    use mudu::common::result::RS;
    use mudu::error::ErrorCode;
    use mudu_contract::procedure::proc_desc::ProcDesc;
    use mudu_contract::procedure::procedure_param::ProcedureParam;
    use mudu_contract::procedure::procedure_result::ProcedureResult;
    use mudu_contract::tuple::tuple_datum::TupleDatum;
    use mudu_sys::contract::async_io_provider::AsyncIoProvider;
    use serde_json::{Map, Value};
    use std::io::{Cursor, Write};
    use std::sync::Arc;

    use crate::service::app_inst::AppInst;
    use crate::service::runtime::Runtime;

    fn sample_desc() -> ProcDesc {
        ProcDesc::new(
            "mod1".to_string(),
            "proc1".to_string(),
            <(i32,)>::tuple_desc_static(&["value".to_string()]),
            <(i32,)>::tuple_desc_static(&["value".to_string()]),
            false,
        )
    }

    struct MockAppInst {
        info: AppInfo,
        desc: ProcDesc,
    }

    #[async_trait]
    impl AppInst for MockAppInst {
        fn cfg(&self) -> &AppInfo {
            &self.info
        }

        async fn task_create(&self) -> RS<mudu_utils::task_id::TaskID> {
            Ok(1)
        }

        fn task_end(&self, _task_id: mudu_utils::task_id::TaskID) -> RS<()> {
            Ok(())
        }

        fn connection(
            &self,
            _task_id: mudu_utils::task_id::TaskID,
        ) -> Option<mudu_contract::database::sql::DBConn> {
            None
        }

        fn procedure(&self) -> RS<Vec<(String, String)>> {
            Ok(vec![("mod1".to_string(), "proc1".to_string())])
        }

        async fn invoke(
            &self,
            _task_id: mudu_utils::task_id::TaskID,
            _mod_name: &str,
            _proc_name: &str,
            _param: ProcedureParam,
            _worker_local: Option<mudu_kernel::server::worker_local::WorkerLocalRef>,
        ) -> RS<ProcedureResult> {
            let desc = sample_desc();
            Ok(ProcedureResult::from(Ok((42i32,)), desc.return_desc())?)
        }

        async fn invoke_async(
            &self,
            _task_id: mudu_utils::task_id::TaskID,
            _mod_name: &str,
            _proc_name: &str,
            _param: ProcedureParam,
            _worker_local: Option<mudu_kernel::server::worker_local::WorkerLocalRef>,
        ) -> RS<ProcedureResult> {
            let desc = sample_desc();
            Ok(ProcedureResult::from(Ok((42i32,)), desc.return_desc())?)
        }

        fn describe(&self, _mod_name: &str, _proc_name: &str) -> RS<Arc<ProcDesc>> {
            Ok(Arc::new(self.desc.clone()))
        }
    }

    struct MockRuntime {
        app: Arc<MockAppInst>,
    }

    #[async_trait]
    impl Runtime for MockRuntime {
        async fn list(&self) -> Vec<String> {
            vec!["app1".to_string()]
        }

        async fn app(&self, _app_name: String) -> Option<Arc<dyn AppInst>> {
            Some(self.app.clone())
        }

        async fn install(&self, _pkg_path: String) -> RS<()> {
            Ok(())
        }

        fn async_runtime(&self) -> Option<Arc<dyn AsyncIoProvider>> {
            None
        }
    }

    fn mock_runtime(use_async: bool) -> Arc<MockRuntime> {
        Arc::new(MockRuntime {
            app: Arc::new(MockAppInst {
                info: AppInfo {
                    name: "app1".to_string(),
                    lang: "rust".to_string(),
                    version: "0.1.0".to_string(),
                    use_async,
                },
                desc: sample_desc(),
            }),
        })
    }

    #[test]
    fn decode_install_request_extracts_base64_payload() {
        let payload = b"hello mpk";
        let encoded = base64::engine::general_purpose::STANDARD.encode(payload);
        let body = serde_json::json!({"mpk_base64": encoded}).to_string();
        assert_eq!(decode_install_request(&body).unwrap(), payload.to_vec());
    }

    #[test]
    fn decode_install_request_rejects_missing_key() {
        let err = decode_install_request("{}").unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        assert!(err.message().contains("mpk_base64 missing"));
    }

    #[test]
    fn decode_install_request_rejects_invalid_json() {
        let err = decode_install_request("not json").unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }

    #[test]
    fn decode_install_request_rejects_invalid_base64() {
        let err = decode_install_request(r#"{"mpk_base64": "!!!"}"#).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }

    // Miri flags Stacked-Borrows UB inside zlib-rs (used by zip/flate2) when
    // the DeflateEncoder drops its internal writer, so run this test natively.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn mpk_package_name_reads_package_cfg_json() {
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("package.cfg.json", options).unwrap();
            zip.write_all(br#"{"name":"demo-app"}"#).unwrap();
            zip.finish().unwrap();
        }
        assert_eq!(mpk_package_name(&buf).unwrap(), "demo-app");
    }

    #[test]
    fn mpk_package_name_returns_none_for_invalid_archive() {
        assert!(mpk_package_name(b"not a zip").is_none());
    }

    #[test]
    fn parse_json_object_body_accepts_object() {
        let body = r#"{"a":1}"#;
        let map = parse_json_object_body(body).unwrap();
        assert_eq!(map["a"], 1);
    }

    #[test]
    fn parse_json_object_body_rejects_non_object() {
        let err = parse_json_object_body("[1,2]").unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }

    #[test]
    fn to_param_converts_matching_fields() {
        let desc = sample_desc();
        let mut map = Map::new();
        map.insert("value".to_string(), Value::from(7));
        let param = to_param(&map, desc.param_desc().fields()).unwrap();
        assert_eq!(param.param_list().len(), 1);
    }

    #[test]
    fn to_param_errors_on_missing_field() {
        let desc = sample_desc();
        let map = Map::new();
        let err = to_param(&map, desc.param_desc().fields()).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    }

    #[tokio::test]
    async fn runtime_get_app_and_desc_finds_procedure() {
        let rt = mock_runtime(false);
        let (app, desc) = runtime_get_app_and_desc(rt, "app1", "mod1", "proc1")
            .await
            .unwrap();
        assert_eq!(app.cfg().name, "app1");
        assert_eq!(desc.proc_name(), "proc1");
    }

    #[tokio::test]
    async fn runtime_get_app_and_desc_errors_when_app_missing() {
        struct EmptyRuntime;
        #[async_trait]
        impl Runtime for EmptyRuntime {
            async fn list(&self) -> Vec<String> {
                vec![]
            }
            async fn app(&self, _app_name: String) -> Option<Arc<dyn AppInst>> {
                None
            }
            async fn install(&self, _pkg_path: String) -> RS<()> {
                Ok(())
            }
            fn async_runtime(&self) -> Option<Arc<dyn AsyncIoProvider>> {
                None
            }
        }
        let result =
            runtime_get_app_and_desc(Arc::new(EmptyRuntime), "missing", "mod1", "proc1").await;
        match result {
            Err(err) => assert_eq!(err.ec(), ErrorCode::EntityNotFound),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn legacy_invoke_sync_proc_returns_json_result() {
        let app = mock_runtime(false).app.clone();
        let desc = Arc::new(sample_desc());
        let mut map = Map::new();
        map.insert("value".to_string(), Value::from(1));
        let result = legacy_invoke_sync_proc("mod1", "proc1", map, app, desc)
            .await
            .unwrap()
            .unwrap();
        assert!(result.is_object());
    }

    #[tokio::test]
    async fn legacy_invoke_async_proc_returns_json_result() {
        let app = mock_runtime(true).app.clone();
        let desc = Arc::new(sample_desc());
        let mut map = Map::new();
        map.insert("value".to_string(), Value::from(1));
        let result = legacy_invoke_async_proc("mod1", "proc1", map, app, desc)
            .await
            .unwrap();
        assert!(result.is_object());
    }

    #[tokio::test]
    async fn legacy_http_api_list_apps() {
        let api = LegacyHttpApi::new(mock_runtime(false));
        assert_eq!(api.list_apps().await.unwrap(), vec!["app1"]);
    }

    #[tokio::test]
    async fn legacy_http_api_list_procedures() {
        let api = LegacyHttpApi::new(mock_runtime(false));
        assert_eq!(
            api.list_procedures("app1").await.unwrap(),
            vec!["mod1/proc1"]
        );
    }

    #[tokio::test]
    async fn legacy_http_api_procedure_detail() {
        let api = LegacyHttpApi::new(mock_runtime(false));
        let (desc, param, ret) = api.procedure_detail("app1", "mod1", "proc1").await.unwrap();
        assert_eq!(desc.proc_name(), "proc1");
        assert_eq!(param["value"], 0);
        assert_eq!(ret["value"], 0);
    }

    #[tokio::test]
    async fn legacy_http_api_install_mpk() {
        let api = LegacyHttpApi::new(mock_runtime(false));
        api.install_mpk(vec![1, 2, 3]).await.unwrap();
    }

    #[tokio::test]
    async fn legacy_http_api_invoke_json_sync() {
        let api = LegacyHttpApi::new(mock_runtime(false));
        let result = api
            .invoke_json("app1", "mod1", "proc1", r#"{"value":1}"#.to_string())
            .await
            .unwrap();
        assert!(result.is_object());
    }

    #[tokio::test]
    async fn legacy_http_api_invoke_json_async() {
        let api = LegacyHttpApi::new(mock_runtime(true));
        let result = api
            .invoke_json("app1", "mod1", "proc1", r#"{"value":1}"#.to_string())
            .await
            .unwrap();
        assert!(result.is_object());
    }

    #[tokio::test]
    async fn legacy_http_api_invoke_json_rejects_non_object_body() {
        let api = LegacyHttpApi::new(mock_runtime(false));
        let err = api
            .invoke_json("app1", "mod1", "proc1", "[]".to_string())
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }

    fn legacy_http_api() -> Arc<dyn HttpApi> {
        Arc::new(LegacyHttpApi::new(mock_runtime(false)))
    }

    #[actix_web::test]
    async fn http_app_list_returns_apps() {
        if cfg!(miri) {
            return;
        }
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(HttpApiContext {
                    api: legacy_http_api(),
                }))
                .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::IOURING)),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri("/mudu/app/list")
            .to_request();
        let resp: Value = actix_test::call_and_read_body_json(&app, req).await;
        assert!(resp["ok"].as_bool().unwrap());
        assert_eq!(resp["data"], serde_json::json!(["app1"]));
    }

    #[actix_web::test]
    async fn http_app_proc_list_returns_procedures() {
        if cfg!(miri) {
            return;
        }
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(HttpApiContext {
                    api: legacy_http_api(),
                }))
                .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::IOURING)),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri("/mudu/app/list/app1")
            .to_request();
        let resp: Value = actix_test::call_and_read_body_json(&app, req).await;
        assert!(resp["ok"].as_bool().unwrap());
        assert_eq!(resp["data"]["app_name"], "app1");
    }

    #[actix_web::test]
    async fn http_app_proc_detail_returns_descriptor() {
        if cfg!(miri) {
            return;
        }
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(HttpApiContext {
                    api: legacy_http_api(),
                }))
                .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::IOURING)),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri("/mudu/app/list/app1/mod1/proc1")
            .to_request();
        let resp: Value = actix_test::call_and_read_body_json(&app, req).await;
        assert!(resp["ok"].as_bool().unwrap());
        assert_eq!(resp["data"]["proc_desc"]["proc_name"], "proc1");
    }

    #[actix_web::test]
    async fn http_install_accepts_base64_payload() {
        if cfg!(miri) {
            return;
        }
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(HttpApiContext {
                    api: legacy_http_api(),
                }))
                .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::IOURING)),
        )
        .await;

        let payload = base64::engine::general_purpose::STANDARD.encode(b"mpk");
        let body = serde_json::json!({"mpk_base64": payload}).to_string();
        let req = actix_test::TestRequest::post()
            .uri("/mudu/app/install")
            .set_payload(body)
            .to_request();
        let resp: Value = actix_test::call_and_read_body_json(&app, req).await;
        assert!(resp["ok"].as_bool().unwrap());
    }

    #[actix_web::test]
    async fn http_install_rejects_missing_base64() {
        if cfg!(miri) {
            return;
        }
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(HttpApiContext {
                    api: legacy_http_api(),
                }))
                .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::IOURING)),
        )
        .await;

        let req = actix_test::TestRequest::post()
            .uri("/mudu/app/install")
            .set_payload("{}")
            .to_request();
        let resp: Value = actix_test::call_and_read_body_json(&app, req).await;
        assert!(!resp["ok"].as_bool().unwrap());
    }

    #[actix_web::test]
    async fn http_invoke_routes_to_api() {
        if cfg!(miri) {
            return;
        }
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(HttpApiContext {
                    api: legacy_http_api(),
                }))
                .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::IOURING)),
        )
        .await;

        let req = actix_test::TestRequest::post()
            .uri("/mudu/app/invoke/app1/mod1/proc1")
            .set_payload(r#"{"value":1}"#)
            .to_request();
        let resp: Value = actix_test::call_and_read_body_json(&app, req).await;
        assert!(resp["ok"].as_bool().unwrap());
    }

    #[actix_web::test]
    async fn http_uninstall_returns_not_implemented_for_legacy() {
        if cfg!(miri) {
            return;
        }
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(HttpApiContext {
                    api: legacy_http_api(),
                }))
                .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::IOURING)),
        )
        .await;

        let req = actix_test::TestRequest::delete()
            .uri("/mudu/app/uninstall/app1")
            .to_request();
        let resp: Value = actix_test::call_and_read_body_json(&app, req).await;
        assert!(!resp["ok"].as_bool().unwrap());
    }

    #[actix_web::test]
    async fn http_partition_route_returns_error_on_invalid_json() {
        if cfg!(miri) {
            return;
        }
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(HttpApiContext {
                    api: legacy_http_api(),
                }))
                .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::IOURING)),
        )
        .await;

        let req = actix_test::TestRequest::post()
            .uri("/mudu/partition/route")
            .set_payload("not-json")
            .to_request();
        let resp: Value = actix_test::call_and_read_body_json(&app, req).await;
        assert!(!resp["ok"].as_bool().unwrap());
        assert_eq!(resp["status"], ErrorCode::Decode.to_u32());
    }

    #[actix_web::test]
    async fn http_server_topology_returns_not_implemented_for_legacy() {
        if cfg!(miri) {
            return;
        }
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(HttpApiContext {
                    api: legacy_http_api(),
                }))
                .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::IOURING)),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri("/mudu/server/topology")
            .to_request();
        let resp: Value = actix_test::call_and_read_body_json(&app, req).await;
        assert!(!resp["ok"].as_bool().unwrap());
    }

    #[actix_web::test]
    async fn http_invoke_disabled_by_capabilities() {
        if cfg!(miri) {
            return;
        }
        let disabled = HttpApiCapabilities {
            enable_invoke: false,
            enable_uninstall: false,
        };
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(HttpApiContext {
                    api: legacy_http_api(),
                }))
                .configure(|cfg| configure_routes(cfg, disabled)),
        )
        .await;

        let req = actix_test::TestRequest::post()
            .uri("/mudu/app/invoke/app1/mod1/proc1")
            .set_payload(r#"{"value":1}"#)
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
