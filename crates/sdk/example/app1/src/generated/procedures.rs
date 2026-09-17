//! The three app1 test procedures, ported from the historical `mudu_wasm`
//! guest crate. Their signatures are pinned by `package/package.desc.json`
//! and by the `mudu_runtime` component/procedure tests.

use crate::generated::wallets::object::Wallets;
use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::contract::{sql_params, sql_stmt};
use mududb::sys_interface::async_api::{mudu_command, mudu_query};
use mududb::types::datum::{Datum, DatumDyn};

/**mudu-proc**/
/// Echoes the scalar arguments: returns `(a + b, "xid:.., a=.., b=.., c=..")`.
pub fn proc_mtp(xid: OID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    Ok((
        (a + b as i32),
        format!("xid:{}, a={}, b={}, c={}", xid, a, b, c),
    ))
}

/**mudu-proc**/
/// Same echo behavior as [`proc_mtp`], declared as a second procedure.
pub fn proc2_mtp(xid: OID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    Ok((
        (a + b as i32),
        format!("xid:{}, a={}, b={}, c={}", xid, a, b, c),
    ))
}

/**mudu-proc**/
/// Exercises real host syscalls: creates the wallets table, seeds two rows
/// and returns the query results rendered as text.
pub async fn proc_sys_call_mtp(xid: OID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    let _affected_rows = mudu_command(
        xid,
        &r#"
CREATE TABLE wallets
(
    user_id    INT PRIMARY KEY,
    balance    INT,
    updated_at INT
);"#
        .to_string(),
        &vec![],
    )
    .await?;

    for i in 1..=2 {
        let _affected_rows = mudu_command(
            xid,
            &r#"
INSERT INTO wallets
(
    user_id,
    balance,
    updated_at
) VALUES (
    ?,
    ?,
    ?
)"#
            .to_string(),
            &(i, 100i32, 10000i32),
        )
        .await?;
    }

    let wallet_rs = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT user_id, balance, updated_at FROM wallets;"),
        sql_params!(&()),
    )
    .await?;

    let mut result = String::new();
    while let Some(row) = wallet_rs.next_record()? {
        let value = row.to_value(&Wallets::data_type())?;
        let s = value.to_textual(&Wallets::data_type())?;
        result.push_str(&s);
        result.push('\n');
    }
    Ok((
        (a + b as i32),
        format!("xid:{}, a={}, b={}, c={}, result {}", xid, a, b, c, result),
    ))
}
async fn mp2_proc_sys_call_mtp(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_proc_sys_call_mtp,
    )
    .await
}

pub async fn mudu_inner_p2_proc_sys_call_mtp(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = proc_sys_call_mtp(
        param.session_id(),
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[0], "i32")?,
        ::mududb::types::datum::value_to_typed::<i64, _>(&param.param_list()[1], "i64")?,
        ::mududb::types::datum::value_to_typed::<String, _>(&param.param_list()[2], "String")?,
    )
    .await;
    match res {
        Ok(tuple) => {
            let return_list = {
                let (mudu_ret_0, mudu_ret_1) = tuple;
                vec![
                    ::mududb::types::datum::value_from_typed(&mudu_ret_0, "i32")?,
                    ::mududb::types::datum::value_from_typed(&mudu_ret_1, "String")?,
                ]
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_proc_sys_call_mtp()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "a".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "b".to_string(),
                <i64 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "c".to_string(),
                <String as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
        ])
    })
}

pub fn mudu_result_desc_proc_sys_call_mtp()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "0".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "1".to_string(),
                <String as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
        ])
    })
}

pub fn mudu_proc_desc_proc_sys_call_mtp()
-> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "mod_0".to_string(),
            "proc_sys_call_mtp".to_string(),
            mudu_argv_desc_proc_sys_call_mtp().clone(),
            mudu_result_desc_proc_sys_call_mtp().clone(),
            false,
        )
    })
}

mod mod_proc_sys_call_mtp {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-proc-sys-call-mtp;
            world mudu-app-mp2-proc-sys-call-mtp {
                export mp2-proc-sys-call-mtp: async func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestProcSysCallMtp {}

    impl Guest for GuestProcSysCallMtp {
        async fn mp2_proc_sys_call_mtp(param: Vec<u8>) -> Vec<u8> {
            super::mp2_proc_sys_call_mtp(param).await
        }
    }

    export!(GuestProcSysCallMtp);
}
fn mp2_proc2_mtp(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure(param, mudu_inner_p2_proc2_mtp)
}

pub fn mudu_inner_p2_proc2_mtp(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = proc2_mtp(
        param.session_id(),
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[0], "i32")?,
        ::mududb::types::datum::value_to_typed::<i64, _>(&param.param_list()[1], "i64")?,
        ::mududb::types::datum::value_to_typed::<String, _>(&param.param_list()[2], "String")?,
    );
    match res {
        Ok(tuple) => {
            let return_list = {
                let (mudu_ret_0, mudu_ret_1) = tuple;
                vec![
                    ::mududb::types::datum::value_from_typed(&mudu_ret_0, "i32")?,
                    ::mududb::types::datum::value_from_typed(&mudu_ret_1, "String")?,
                ]
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_proc2_mtp()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "a".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "b".to_string(),
                <i64 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "c".to_string(),
                <String as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
        ])
    })
}

pub fn mudu_result_desc_proc2_mtp()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "0".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "1".to_string(),
                <String as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
        ])
    })
}

pub fn mudu_proc_desc_proc2_mtp() -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "mod_0".to_string(),
            "proc2_mtp".to_string(),
            mudu_argv_desc_proc2_mtp().clone(),
            mudu_result_desc_proc2_mtp().clone(),
            false,
        )
    })
}

mod mod_proc2_mtp {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-proc2-mtp;
            world mudu-app-mp2-proc2-mtp {
                export mp2-proc2-mtp:  func(param:list<u8>) -> list<u8>;
            }
        "##,

    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestProc2Mtp {}

    impl Guest for GuestProc2Mtp {
        fn mp2_proc2_mtp(param: Vec<u8>) -> Vec<u8> {
            super::mp2_proc2_mtp(param)
        }
    }

    export!(GuestProc2Mtp);
}
fn mp2_proc_mtp(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure(param, mudu_inner_p2_proc_mtp)
}

pub fn mudu_inner_p2_proc_mtp(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = proc_mtp(
        param.session_id(),
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[0], "i32")?,
        ::mududb::types::datum::value_to_typed::<i64, _>(&param.param_list()[1], "i64")?,
        ::mududb::types::datum::value_to_typed::<String, _>(&param.param_list()[2], "String")?,
    );
    match res {
        Ok(tuple) => {
            let return_list = {
                let (mudu_ret_0, mudu_ret_1) = tuple;
                vec![
                    ::mududb::types::datum::value_from_typed(&mudu_ret_0, "i32")?,
                    ::mududb::types::datum::value_from_typed(&mudu_ret_1, "String")?,
                ]
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_proc_mtp()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "a".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "b".to_string(),
                <i64 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "c".to_string(),
                <String as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
        ])
    })
}

pub fn mudu_result_desc_proc_mtp()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "0".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "1".to_string(),
                <String as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
        ])
    })
}

pub fn mudu_proc_desc_proc_mtp() -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "mod_0".to_string(),
            "proc_mtp".to_string(),
            mudu_argv_desc_proc_mtp().clone(),
            mudu_result_desc_proc_mtp().clone(),
            false,
        )
    })
}

mod mod_proc_mtp {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-proc-mtp;
            world mudu-app-mp2-proc-mtp {
                export mp2-proc-mtp:  func(param:list<u8>) -> list<u8>;
            }
        "##,

    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestProcMtp {}

    impl Guest for GuestProcMtp {
        fn mp2_proc_mtp(param: Vec<u8>) -> Vec<u8> {
            super::mp2_proc_mtp(param)
        }
    }

    export!(GuestProcMtp);
}
