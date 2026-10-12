use crate::backend::app_mgr::AppMgr;
use crate::backend::mudu_app_mgr::ListOption;
use base64::Engine;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::procedure::procedure_invoke;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::tuple::datum_desc::DatumDesc;
use runtime_wasm::service::app_inst::AppInst;
use runtime_wasm::service::app_list::AppListItem;
use runtime_wasm::service::runtime::Runtime;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::Arc;

pub(crate) fn decode_install_request(body_str: &str) -> RS<Vec<u8>> {
    let map = serde_json::from_str::<HashMap<String, String>>(body_str)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "deserialize body error: {}", e))?;
    let mpk_base64 = map.get("mpk_base64").ok_or_else(|| {
        mudu_error!(
            ErrorCode::InvalidArgument,
            "mpk_base64 missing for install request"
        )
    })?;
    base64::engine::general_purpose::STANDARD
        .decode(mpk_base64)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode error", e))
}

pub(crate) fn mpk_package_name(binary: &[u8]) -> Option<String> {
    let cursor = Cursor::new(binary);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    let mut package_cfg = String::new();
    archive
        .by_name("package.cfg.json")
        .ok()?
        .read_to_string(&mut package_cfg)
        .ok()?;
    serde_json::from_str::<Value>(&package_cfg)
        .ok()?
        .get("name")?
        .as_str()
        .map(str::to_string)
}

pub(crate) fn to_param(argv: &Map<String, Value>, desc: &[DatumDesc]) -> RS<ProcedureParam> {
    let mut vec = vec![];
    for datum_desc in desc.iter() {
        let value = argv
            .get(datum_desc.name())
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("no parameter {}", datum_desc.name())
                )
            })?
            .clone();
        let id = datum_desc.type_family();
        let data_value = id.fn_input_json()(&value, datum_desc.data_type()).map_err(|e| {
            mudu_error!(
                ErrorCode::TypeConversionFailed,
                "convert printable to internal error",
                e
            )
        })?;
        vec.push(data_value)
    }
    Ok(ProcedureParam::new(0, 0, vec))
}

pub(crate) fn parse_json_object_body(body: &str) -> RS<Map<String, Value>> {
    let object: Value = serde_json::from_str(body)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "deserialize error", e))?;
    match object {
        Value::Object(obj_map) => Ok(obj_map),
        _ => Err(mudu_error!(
            ErrorCode::Decode,
            "request json body must be an object"
        )),
    }
}

pub(crate) async fn runtime_get_app_and_desc(
    service: Arc<dyn Runtime>,
    app_name: &str,
    mod_name: &str,
    proc_name: &str,
) -> RS<(Arc<dyn AppInst>, Arc<ProcDesc>)> {
    let app = service.app(app_name.to_string()).await.ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("no such app {}", app_name)
        )
    })?;
    let desc = app.describe(mod_name, proc_name)?;
    Ok((app, desc))
}

pub(crate) async fn legacy_invoke_sync_proc(
    mod_name: &str,
    proc_name: &str,
    argv: Map<String, Value>,
    app: Arc<dyn AppInst>,
    desc: Arc<ProcDesc>,
) -> RS<RS<Value>> {
    let task_id = app.task_create().await?;
    let _app = app.clone();
    let _g = scopeguard::guard(task_id, |task_id| {
        let _r = _app.task_end(task_id);
    });

    let param = to_param(&argv, desc.param_desc().fields())?;
    let result = app
        .invoke(task_id, mod_name, proc_name, param, None)
        .await?;
    Ok(procedure_invoke::result_to_json(result))
}

pub(crate) async fn legacy_invoke_async_proc(
    mod_name: &str,
    proc_name: &str,
    argv: Map<String, Value>,
    app: Arc<dyn AppInst>,
    desc: Arc<ProcDesc>,
) -> RS<Value> {
    let task_id = app.task_create().await?;
    let _g = scopeguard::guard(task_id, |task_id| {
        let _r = app.task_end(task_id);
    });
    let param = to_param(&argv, desc.param_desc().fields())?;
    let result = app
        .invoke_async(task_id, mod_name, proc_name, param, None)
        .await?;
    procedure_invoke::result_to_json(result)
}

pub(crate) async fn find_app(app_mgr: &dyn AppMgr, app_name: &str) -> RS<AppListItem> {
    let listed_apps = app_mgr
        .list(&ListOption {
            names: vec![app_name.to_string()],
        })
        .await?;
    listed_apps
        .apps
        .into_iter()
        .find(|app| app.info.name == app_name)
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no such app {}", app_name)
            )
        })
}
