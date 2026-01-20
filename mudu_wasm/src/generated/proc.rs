use mudu::common::result::RS;
use mudu::common::xid::XID;

/**mudu-proc**/
pub fn proc_mtp(xid: XID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    Ok(((a + b as i32), format!("xid:{}, a={}, b={}, c={}", xid, a, b, c)))
}




 fn mp2_proc_mtp(param:Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure(
        param,
        mudu_inner_p2_proc_mtp,
    )
}

pub  fn mudu_inner_p2_proc_mtp(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_proc_mtp().clone();
    let res = proc_mtp(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
        
            ::mudu_type::datum::value_to_typed::<
                i64,
                _,
            >(&param.param_list()[1], "i64")?,
        
            ::mudu_type::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[2], "String")?,
        
    );
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_proc_mtp()  -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                
                    i32,
                
                    i64,
                
                    String,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                            
                                "a",
                            
                                "b",
                            
                                "c",
                            

                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_proc_mtp() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                
                    i32,
                
                    String,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_proc_mtp()  -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "proc_mtp".to_string(),
                mudu_argv_desc_proc_mtp().clone(),
                mudu_result_desc_proc_mtp().clone(),
                false
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
         fn mp2_proc_mtp(param:Vec<u8>) -> Vec<u8> {
            super::mp2_proc_mtp(param)
        }
    }

    export!(GuestProcMtp);
}