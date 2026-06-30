use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::error::ErrorCode;
use mududb::mudu_error;
use mududb::sys_interface::async_api::{mudu_get, mudu_put, mudu_range};

fn kv_data_key(user_key: &str) -> String {
    format!("user/{user_key}")
}

fn decode_utf8(label: &str, bytes: Vec<u8>) -> RS<String> {
    String::from_utf8(bytes).map_err(|e| {
        mudu_error!(
            ErrorCode::Decode,
            format!("invalid utf8 in key-value {label}"),
            e.to_string()
        )
    })
}

async fn read_value(session_id: OID, user_key: &str) -> RS<String> {
    let key = kv_data_key(user_key);
    let value = mudu_get(session_id, key.as_bytes()).await?.ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("key-value key not found: {user_key}")
        )
    })?;
    decode_utf8("value", value)
}

/**mudu-proc**/
pub async fn kv_insert(xid: OID, user_key: String, value: String) -> RS<()> {
    let key = kv_data_key(&user_key);
    mudu_put(xid, key.as_bytes(), value.as_bytes()).await
}

/**mudu-proc**/
pub async fn kv_read(xid: OID, user_key: String) -> RS<String> {
    read_value(xid, &user_key).await
}

/**mudu-proc**/
pub async fn kv_update(xid: OID, user_key: String, value: String) -> RS<()> {
    let key = kv_data_key(&user_key);
    let _ = mudu_get(xid, key.as_bytes()).await?.ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("key-value key not found: {user_key}")
        )
    })?;
    mudu_put(xid, key.as_bytes(), value.as_bytes()).await
}

/**mudu-proc**/
pub async fn kv_scan(xid: OID, start_user_key: String, end_user_key: String) -> RS<Vec<String>> {
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
pub async fn kv_read_modify_write(xid: OID, user_key: String, append_value: String) -> RS<String> {
    let key = kv_data_key(&user_key);
    let mut current = match mudu_get(xid, key.as_bytes()).await? {
        Some(value) => decode_utf8("value", value)?,
        None => String::new(),
    };
    current.push_str(&append_value);
    mudu_put(xid, key.as_bytes(), current.as_bytes()).await?;
    Ok(current)
}

async fn mp2_kv_insert(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_kv_insert,
    )
    .await
}

pub async fn mudu_inner_p2_kv_insert(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = kv_insert(
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

pub fn mudu_argv_desc_kv_insert()
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

pub fn mudu_result_desc_kv_insert()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    RESULT_DESC
        .get_or_init(|| ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![]))
}

pub fn mudu_proc_desc_kv_insert() -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "key_value".to_string(),
            "kv_insert".to_string(),
            mudu_argv_desc_kv_insert().clone(),
            mudu_result_desc_kv_insert().clone(),
            false,
        )
    })
}

mod mod_kv_insert {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-kv-insert;
            world mudu-app-mp2-kv-insert {
                export mp2-kv-insert: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestKvInsert {}

    impl Guest for GuestKvInsert {
        async fn mp2_kv_insert(param: Vec<u8>) -> Vec<u8> {
            super::mp2_kv_insert(param).await
        }
    }

    export!(GuestKvInsert);
}

async fn mp2_kv_update(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_kv_update,
    )
    .await
}

pub async fn mudu_inner_p2_kv_update(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = kv_update(
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

pub fn mudu_argv_desc_kv_update()
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

pub fn mudu_result_desc_kv_update()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    RESULT_DESC
        .get_or_init(|| ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![]))
}

pub fn mudu_proc_desc_kv_update() -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "key_value".to_string(),
            "kv_update".to_string(),
            mudu_argv_desc_kv_update().clone(),
            mudu_result_desc_kv_update().clone(),
            false,
        )
    })
}

mod mod_kv_update {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-kv-update;
            world mudu-app-mp2-kv-update {
                export mp2-kv-update: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestKvUpdate {}

    impl Guest for GuestKvUpdate {
        async fn mp2_kv_update(param: Vec<u8>) -> Vec<u8> {
            super::mp2_kv_update(param).await
        }
    }

    export!(GuestKvUpdate);
}

async fn mp2_kv_scan(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_kv_scan,
    )
    .await
}

pub async fn mudu_inner_p2_kv_scan(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = kv_scan(
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

pub fn mudu_argv_desc_kv_scan()
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

pub fn mudu_result_desc_kv_scan()
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

pub fn mudu_proc_desc_kv_scan() -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "key_value".to_string(),
            "kv_scan".to_string(),
            mudu_argv_desc_kv_scan().clone(),
            mudu_result_desc_kv_scan().clone(),
            false,
        )
    })
}

mod mod_kv_scan {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-kv-scan;
            world mudu-app-mp2-kv-scan {
                export mp2-kv-scan: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestKvScan {}

    impl Guest for GuestKvScan {
        async fn mp2_kv_scan(param: Vec<u8>) -> Vec<u8> {
            super::mp2_kv_scan(param).await
        }
    }

    export!(GuestKvScan);
}

async fn mp2_kv_read(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_kv_read,
    )
    .await
}

pub async fn mudu_inner_p2_kv_read(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = kv_read(
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

pub fn mudu_argv_desc_kv_read()
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

pub fn mudu_result_desc_kv_read()
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

pub fn mudu_proc_desc_kv_read() -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "key_value".to_string(),
            "kv_read".to_string(),
            mudu_argv_desc_kv_read().clone(),
            mudu_result_desc_kv_read().clone(),
            false,
        )
    })
}

mod mod_kv_read {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-kv-read;
            world mudu-app-mp2-kv-read {
                export mp2-kv-read: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestKvRead {}

    impl Guest for GuestKvRead {
        async fn mp2_kv_read(param: Vec<u8>) -> Vec<u8> {
            super::mp2_kv_read(param).await
        }
    }

    export!(GuestKvRead);
}

async fn mp2_kv_read_modify_write(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_kv_read_modify_write,
    )
    .await
}

pub async fn mudu_inner_p2_kv_read_modify_write(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = kv_read_modify_write(
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

pub fn mudu_argv_desc_kv_read_modify_write()
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

pub fn mudu_result_desc_kv_read_modify_write()
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

pub fn mudu_proc_desc_kv_read_modify_write()
-> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "key_value".to_string(),
            "kv_read_modify_write".to_string(),
            mudu_argv_desc_kv_read_modify_write().clone(),
            mudu_result_desc_kv_read_modify_write().clone(),
            false,
        )
    })
}

mod mod_kv_read_modify_write {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-kv-read-modify-write;
            world mudu-app-mp2-kv-read-modify-write {
                export mp2-kv-read-modify-write: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestKvReadModifyWrite {}

    impl Guest for GuestKvReadModifyWrite {
        async fn mp2_kv_read_modify_write(param: Vec<u8>) -> Vec<u8> {
            super::mp2_kv_read_modify_write(param).await
        }
    }

    export!(GuestKvReadModifyWrite);
}
