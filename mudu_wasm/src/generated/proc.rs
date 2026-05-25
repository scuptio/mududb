use mududb::common::result::RS;
use mududb::common::xid::XID;

/**mudu-proc**/
pub fn proc_mtp(xid: XID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    Ok((
        (a + b as i32),
        format!("xid:{}, a={}, b={}, c={}", xid, a, b, c),
    ))
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
                <i32 as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "b".to_string(),
                <i64 as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "c".to_string(),
                <String as ::mududb::types::datum::Datum>::dat_type().clone(),
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
                <i32 as ::mududb::types::datum::Datum>::dat_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "1".to_string(),
                <String as ::mududb::types::datum::Datum>::dat_type().clone(),
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
                export mp2-proc-mtp: func(param:list<u8>) -> list<u8>;
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
