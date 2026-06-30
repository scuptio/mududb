use crate::generated::procedure_common::{decode_utf8, kv_data_key};
use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::error::ErrorCode;
use mududb::mudu_error;
use mududb::sys_interface::async_api::{mudu_get, mudu_put, mudu_range};

async fn read_value(session_id: OID, user_key: &str) -> RS<String> {
    let key = kv_data_key(user_key);
    let value = mudu_get(session_id, key.as_bytes()).await?.ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("ycsb key not found: {user_key}")
        )
    })?;
    decode_utf8("value", value)
}

/**mudu-proc**/
pub async fn ycsb_insert(xid: OID, user_key: String, value: String) -> RS<()> {
    let key = kv_data_key(&user_key);
    mudu_put(xid, key.as_bytes(), value.as_bytes()).await
}

/**mudu-proc**/
pub async fn ycsb_read(xid: OID, user_key: String) -> RS<String> {
    read_value(xid, &user_key).await
}

/**mudu-proc**/
pub async fn ycsb_update(xid: OID, user_key: String, value: String) -> RS<()> {
    let key = kv_data_key(&user_key);
    let _ = mudu_get(xid, key.as_bytes()).await?.ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("ycsb key not found: {user_key}")
        )
    })?;
    mudu_put(xid, key.as_bytes(), value.as_bytes()).await
}

/**mudu-proc**/
pub async fn ycsb_scan(xid: OID, start_user_key: String, end_user_key: String) -> RS<Vec<String>> {
    let start_key = kv_data_key(&start_user_key);
    let end_key = kv_data_key(&end_user_key);
    let pairs = mudu_range(xid, start_key.as_bytes(), end_key.as_bytes()).await?;
    let mut rows = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        let decoded_key = decode_utf8("scan key", key)?;
        let decoded_value = decode_utf8("scan value", value)?;
        rows.push(format!("{decoded_key}={decoded_value}"));
    }
    Ok(rows)
}

/**mudu-proc**/
pub async fn ycsb_read_modify_write(
    xid: OID,
    user_key: String,
    append_value: String,
) -> RS<String> {
    let key = kv_data_key(&user_key);
    let mut current = match mudu_get(xid, key.as_bytes()).await? {
        Some(value) => decode_utf8("value", value)?,
        None => String::new(),
    };
    current.push_str(&append_value);
    mudu_put(xid, key.as_bytes(), current.as_bytes()).await?;
    Ok(current)
}

// Miri cannot execute FFI calls into SQLite (via rusqlite), so skip
// these tests under Miri. They are still exercised by normal `cargo test`.
#[cfg(test)]
mod tests {
    use super::{ycsb_insert, ycsb_read, ycsb_read_modify_write, ycsb_scan, ycsb_update};
    use crate::test_lock;
    use mududb::sys_interface::async_api::{mudu_close, mudu_open};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("ycsb_{name}_{suffix}.db"))
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    async fn ycsb_sync_procedures_roundtrip_against_standalone_adapter() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let db_path = temp_db_path("sync");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let xid = mudu_open().await.unwrap();
        ycsb_insert(xid, "u1".to_string(), "v1".to_string())
            .await
            .unwrap();
        ycsb_insert(xid, "u2".to_string(), "v2".to_string())
            .await
            .unwrap();

        assert_eq!(ycsb_read(xid, "u1".to_string()).unwrap(), "v1");

        ycsb_update(xid, "u1".to_string(), "v3".to_string())
            .await
            .unwrap();
        assert_eq!(ycsb_read(xid, "u1".to_string()).unwrap(), "v3");

        assert_eq!(
            ycsb_scan(xid, "u1".to_string(), "uz".to_string()).unwrap(),
            vec!["user/u1=v3".to_string(), "user/u2=v2".to_string()]
        );

        assert_eq!(
            ycsb_read_modify_write(xid, "u1".to_string(), "-x".to_string()).unwrap(),
            "v3-x"
        );

        mudu_close(xid).await.unwrap();
    }
}
async fn mp2_ycsb_read(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_ycsb_read,
    )
    .await
}

pub async fn mudu_inner_p2_ycsb_read(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = ycsb_read(
        param.session_id(),
        ::mududb::types::datum::value_to_typed::<String, _>(&param.param_list()[0], "String")?,
    )
    .await;
    match res {
        Ok(tuple) => {
            let return_list = { vec![::mududb::types::datum::value_from_typed(&tuple, "String")?] };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_ycsb_read()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "user_key".to_string(),
                <String as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
        ])
    })
}

pub fn mudu_result_desc_ycsb_read()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "0".to_string(),
                <String as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
        ])
    })
}

pub fn mudu_proc_desc_ycsb_read() -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "ycsb".to_string(),
            "ycsb_read".to_string(),
            mudu_argv_desc_ycsb_read().clone(),
            mudu_result_desc_ycsb_read().clone(),
            false,
        )
    })
}

mod mod_ycsb_read {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-ycsb-read;
            world mudu-app-mp2-ycsb-read {
                export mp2-ycsb-read: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestYcsbRead {}

    impl Guest for GuestYcsbRead {
        async fn mp2_ycsb_read(param: Vec<u8>) -> Vec<u8> {
            super::mp2_ycsb_read(param).await
        }
    }

    export!(GuestYcsbRead);
}

async fn mp2_ycsb_update(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_ycsb_update,
    )
    .await
}

pub async fn mudu_inner_p2_ycsb_update(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = ycsb_update(
        param.session_id(),
        ::mududb::types::datum::value_to_typed::<String, _>(&param.param_list()[0], "String")?,
        ::mududb::types::datum::value_to_typed::<String, _>(&param.param_list()[1], "String")?,
    )
    .await;
    match res {
        Ok(tuple) => {
            let return_list = { vec![] };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_ycsb_update()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "user_key".to_string(),
                <String as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "value".to_string(),
                <String as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
        ])
    })
}

pub fn mudu_result_desc_ycsb_update()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    RESULT_DESC
        .get_or_init(|| ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![]))
}

pub fn mudu_proc_desc_ycsb_update() -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "ycsb".to_string(),
            "ycsb_update".to_string(),
            mudu_argv_desc_ycsb_update().clone(),
            mudu_result_desc_ycsb_update().clone(),
            false,
        )
    })
}

mod mod_ycsb_update {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-ycsb-update;
            world mudu-app-mp2-ycsb-update {
                export mp2-ycsb-update: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestYcsbUpdate {}

    impl Guest for GuestYcsbUpdate {
        async fn mp2_ycsb_update(param: Vec<u8>) -> Vec<u8> {
            super::mp2_ycsb_update(param).await
        }
    }

    export!(GuestYcsbUpdate);
}

async fn mp2_ycsb_insert(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_ycsb_insert,
    )
    .await
}

pub async fn mudu_inner_p2_ycsb_insert(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = ycsb_insert(
        param.session_id(),
        ::mududb::types::datum::value_to_typed::<String, _>(&param.param_list()[0], "String")?,
        ::mududb::types::datum::value_to_typed::<String, _>(&param.param_list()[1], "String")?,
    )
    .await;
    match res {
        Ok(tuple) => {
            let return_list = { vec![] };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_ycsb_insert()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "user_key".to_string(),
                <String as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "value".to_string(),
                <String as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
        ])
    })
}

pub fn mudu_result_desc_ycsb_insert()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    RESULT_DESC
        .get_or_init(|| ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![]))
}

pub fn mudu_proc_desc_ycsb_insert() -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "ycsb".to_string(),
            "ycsb_insert".to_string(),
            mudu_argv_desc_ycsb_insert().clone(),
            mudu_result_desc_ycsb_insert().clone(),
            false,
        )
    })
}

mod mod_ycsb_insert {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-ycsb-insert;
            world mudu-app-mp2-ycsb-insert {
                export mp2-ycsb-insert: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestYcsbInsert {}

    impl Guest for GuestYcsbInsert {
        async fn mp2_ycsb_insert(param: Vec<u8>) -> Vec<u8> {
            super::mp2_ycsb_insert(param).await
        }
    }

    export!(GuestYcsbInsert);
}

async fn mp2_ycsb_read_modify_write(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_ycsb_read_modify_write,
    )
    .await
}

pub async fn mudu_inner_p2_ycsb_read_modify_write(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = ycsb_read_modify_write(
        param.session_id(),
        ::mududb::types::datum::value_to_typed::<String, _>(&param.param_list()[0], "String")?,
        ::mududb::types::datum::value_to_typed::<String, _>(&param.param_list()[1], "String")?,
    )
    .await;
    match res {
        Ok(tuple) => {
            let return_list = { vec![::mududb::types::datum::value_from_typed(&tuple, "String")?] };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_ycsb_read_modify_write()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "user_key".to_string(),
                <String as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "append_value".to_string(),
                <String as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
        ])
    })
}

pub fn mudu_result_desc_ycsb_read_modify_write()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "0".to_string(),
                <String as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
        ])
    })
}

pub fn mudu_proc_desc_ycsb_read_modify_write()
-> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "ycsb".to_string(),
            "ycsb_read_modify_write".to_string(),
            mudu_argv_desc_ycsb_read_modify_write().clone(),
            mudu_result_desc_ycsb_read_modify_write().clone(),
            false,
        )
    })
}

mod mod_ycsb_read_modify_write {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-ycsb-read-modify-write;
            world mudu-app-mp2-ycsb-read-modify-write {
                export mp2-ycsb-read-modify-write: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestYcsbReadModifyWrite {}

    impl Guest for GuestYcsbReadModifyWrite {
        async fn mp2_ycsb_read_modify_write(param: Vec<u8>) -> Vec<u8> {
            super::mp2_ycsb_read_modify_write(param).await
        }
    }

    export!(GuestYcsbReadModifyWrite);
}

async fn mp2_ycsb_scan(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_ycsb_scan,
    )
    .await
}

pub async fn mudu_inner_p2_ycsb_scan(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = ycsb_scan(
        param.session_id(),
        ::mududb::types::datum::value_to_typed::<String, _>(&param.param_list()[0], "String")?,
        ::mududb::types::datum::value_to_typed::<String, _>(&param.param_list()[1], "String")?,
    )
    .await;
    match res {
        Ok(tuple) => {
            let return_list = {
                vec![::mududb::types::datum::value_from_typed(
                    &tuple,
                    "Vec<String, >",
                )?]
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_ycsb_scan()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "start_user_key".to_string(),
                <String as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "end_user_key".to_string(),
                <String as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
        ])
    })
}

pub fn mudu_result_desc_ycsb_scan()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "0".to_string(),
                <Vec<String> as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
        ])
    })
}

pub fn mudu_proc_desc_ycsb_scan() -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "ycsb".to_string(),
            "ycsb_scan".to_string(),
            mudu_argv_desc_ycsb_scan().clone(),
            mudu_result_desc_ycsb_scan().clone(),
            false,
        )
    })
}

mod mod_ycsb_scan {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-ycsb-scan;
            world mudu-app-mp2-ycsb-scan {
                export mp2-ycsb-scan: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestYcsbScan {}

    impl Guest for GuestYcsbScan {
        async fn mp2_ycsb_scan(param: Vec<u8>) -> Vec<u8> {
            super::mp2_ycsb_scan(param).await
        }
    }

    export!(GuestYcsbScan);
}
