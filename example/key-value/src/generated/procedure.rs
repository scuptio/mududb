use mududb::common::result::RS;
use mududb::common::xid::XID;
use mududb::error::ec::EC;
use mududb::m_error;
use mududb::sys_interface::async_api::{mudu_get, mudu_put, mudu_range};

fn kv_data_key(user_key: &str) -> String {
    format!("user/{user_key}")
}

fn decode_utf8(label: &str, bytes: Vec<u8>) -> RS<String> {
    String::from_utf8(bytes).map_err(|e| {
        m_error!(
            EC::DecodeErr,
            format!("invalid utf8 in key-value {label}"),
            e.to_string()
        )
    })
}

async fn read_value(session_id: XID, user_key: &str) -> RS<String> {
    let key = kv_data_key(user_key);
    let value = mudu_get(session_id, key.as_bytes()).await?
        .ok_or_else(|| m_error!(EC::NoneErr, format!("key-value key not found: {user_key}")))?;
    decode_utf8("value", value)
}

/**mudu-proc**/
pub async fn kv_insert(xid: XID, user_key: String, value: String) -> RS<()> {
    let key = kv_data_key(&user_key);
    mudu_put(xid, key.as_bytes(), value.as_bytes()).await
}

/**mudu-proc**/
pub async fn kv_read(xid: XID, user_key: String) -> RS<String> {
    read_value(xid, &user_key).await
}

/**mudu-proc**/
pub async fn kv_update(xid: XID, user_key: String, value: String) -> RS<()> {
    let key = kv_data_key(&user_key);
    let _ = mudu_get(xid, key.as_bytes()).await?
        .ok_or_else(|| m_error!(EC::NoneErr, format!("key-value key not found: {user_key}")))?;
    mudu_put(xid, key.as_bytes(), value.as_bytes()).await
}

/**mudu-proc**/
pub async fn kv_scan(xid: XID, start_user_key: String, end_user_key: String) -> RS<Vec<String>> {
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
pub async fn kv_read_modify_write(xid: XID, user_key: String, append_value: String) -> RS<String> {
    let key = kv_data_key(&user_key);
    let mut current = match mudu_get(xid, key.as_bytes()).await? {
        Some(value) => decode_utf8("value", value)?,
        None => String::new(),
    };
    current.push_str(&append_value);
    mudu_put(xid, key.as_bytes(), current.as_bytes()).await?;
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::{kv_insert, kv_read, kv_read_modify_write, kv_scan, kv_update};
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};
    use mududb::sys_interface::async_api::{mudu_close, mudu_open};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_db_path(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("key_value_{name}_{suffix}.db"))
    }

    #[test]
    async fn key_value_procedures_roundtrip_against_standalone_adapter() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let db_path = temp_db_path("roundtrip");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let xid = mudu_open().await.unwrap();
        kv_insert(xid, "a".to_string(), "1".to_string()).await.unwrap();
        kv_insert(xid, "b".to_string(), "2".to_string()).await.unwrap();

        assert_eq!(kv_read(xid, "a".to_string()).unwrap(), "1");

        kv_update(xid, "a".to_string(), "3".to_string()).await.unwrap();
        assert_eq!(kv_read(xid, "a".to_string()).unwrap(), "3");

        let rows = kv_scan(xid, "a".to_string(), "z".to_string()).await.unwrap();
        assert_eq!(rows, vec!["user/a=3".to_string(), "user/b=2".to_string()]);

        let updated = kv_read_modify_write(xid, "a".to_string(), "-tail".to_string()).await.unwrap();
        assert_eq!(updated, "3-tail");
        assert_eq!(kv_read(xid, "a".to_string()).unwrap(), "3-tail");

        mudu_close(xid).await.unwrap();
    }

    #[test]
    async fn kv_update_requires_existing_key() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let db_path = temp_db_path("missing");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let xid = mudu_open().await.unwrap();
        let err = kv_update(xid, "missing".to_string(), "x".to_string()).await.unwrap_err();
        assert!(err.message().contains("missing"));
        mudu_close(xid).await.unwrap();
    }
}
async fn mp2_kv_read_modify_write(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_kv_read_modify_write,
    ).await
}

pub async fn mudu_inner_p2_kv_read_modify_write(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = kv_read_modify_write(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[0], "String")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[1], "String")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mududb::types::datum::value_from_typed(&tuple, "String")?
                    
                ]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_kv_read_modify_write()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "user_key".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "append_value".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_kv_read_modify_write() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_kv_read_modify_write()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "key_value".to_string(),
                "kv_read_modify_write".to_string(),
                mudu_argv_desc_kv_read_modify_write().clone(),
                mudu_result_desc_kv_read_modify_write().clone(),
                false
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
        async fn mp2_kv_read_modify_write(param:Vec<u8>) -> Vec<u8> {
            super::mp2_kv_read_modify_write(param).await
        }
    }

    export!(GuestKvReadModifyWrite);
}
async fn mp2_kv_insert(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_kv_insert,
    ).await
}

pub async fn mudu_inner_p2_kv_insert(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = kv_insert(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[0], "String")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[1], "String")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_kv_insert()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "user_key".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "value".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_kv_insert() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
            ])
        }
    )
}

pub fn mudu_proc_desc_kv_insert()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "key_value".to_string(),
                "kv_insert".to_string(),
                mudu_argv_desc_kv_insert().clone(),
                mudu_result_desc_kv_insert().clone(),
                false
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
        async fn mp2_kv_insert(param:Vec<u8>) -> Vec<u8> {
            super::mp2_kv_insert(param).await
        }
    }

    export!(GuestKvInsert);
}
async fn mp2_kv_scan(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_kv_scan,
    ).await
}

pub async fn mudu_inner_p2_kv_scan(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = kv_scan(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[0], "String")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[1], "String")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mududb::types::datum::value_from_typed(&tuple, "Vec<String, >")?
                    
                ]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_kv_scan()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "start_user_key".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "end_user_key".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_kv_scan() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    <Vec<String, > as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_kv_scan()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "key_value".to_string(),
                "kv_scan".to_string(),
                mudu_argv_desc_kv_scan().clone(),
                mudu_result_desc_kv_scan().clone(),
                false
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
        async fn mp2_kv_scan(param:Vec<u8>) -> Vec<u8> {
            super::mp2_kv_scan(param).await
        }
    }

    export!(GuestKvScan);
}
async fn mp2_kv_read(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_kv_read,
    ).await
}

pub async fn mudu_inner_p2_kv_read(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = kv_read(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[0], "String")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mududb::types::datum::value_from_typed(&tuple, "String")?
                    
                ]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_kv_read()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "user_key".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_kv_read() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_kv_read()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "key_value".to_string(),
                "kv_read".to_string(),
                mudu_argv_desc_kv_read().clone(),
                mudu_result_desc_kv_read().clone(),
                false
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
        async fn mp2_kv_read(param:Vec<u8>) -> Vec<u8> {
            super::mp2_kv_read(param).await
        }
    }

    export!(GuestKvRead);
}
async fn mp2_kv_update(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_kv_update,
    ).await
}

pub async fn mudu_inner_p2_kv_update(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = kv_update(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[0], "String")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[1], "String")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_kv_update()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "user_key".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "value".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_kv_update() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
            ])
        }
    )
}

pub fn mudu_proc_desc_kv_update()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "key_value".to_string(),
                "kv_update".to_string(),
                mudu_argv_desc_kv_update().clone(),
                mudu_result_desc_kv_update().clone(),
                false
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
        async fn mp2_kv_update(param:Vec<u8>) -> Vec<u8> {
            super::mp2_kv_update(param).await
        }
    }

    export!(GuestKvUpdate);
}