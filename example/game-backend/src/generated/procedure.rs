use mudu::common::id::OID;
use mudu::common::result::RS;

/**mudu-proc**/
pub fn command(oid:OID, message: Vec<u8>) -> RS<Vec<u8>> {
    Ok(message)
}

/**mudu-proc**/
pub fn event(oid:OID) -> RS<Vec<u8>> {
    Ok(Vec::new())
}
 fn mp2_event(param:Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure(
        param,
        mudu_inner_p2_event,
    )
}

pub  fn mudu_inner_p2_event(
    param: ::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_event().clone();
    let res = event(
        param.session_id(),
        
    );
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mudu_type::dat_value::DatValue::from_binary(tuple)
                    
                ]
                
            };
            Ok(::mudu_contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_event()  -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
            ])
        }
    )
}

pub fn mudu_result_desc_event() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mudu_contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    ::mudu_type::dat_type::DatType::new_no_param(::mudu_type::dat_type_id::DatTypeID::Binary)
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_event()  -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "game_backend".to_string(),
                "event".to_string(),
                mudu_argv_desc_event().clone(),
                mudu_result_desc_event().clone(),
                false
            )
        })
}

mod mod_event {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-event;
            world mudu-app-mp2-event {
                export mp2-event: func(param:list<u8>) -> list<u8>;
            }
        "##,
        
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestEvent {}

    impl Guest for GuestEvent {
         fn mp2_event(param:Vec<u8>) -> Vec<u8> {
            super::mp2_event(param)
        }
    }

    export!(GuestEvent);
}
 fn mp2_command(param:Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure(
        param,
        mudu_inner_p2_command,
    )
}

pub  fn mudu_inner_p2_command(
    param: ::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_command().clone();
    let res = command(
        param.session_id(),
        
            
            param.param_list()[0].expect_binary().clone(),
            
        
    );
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mudu_type::dat_value::DatValue::from_binary(tuple)
                    
                ]
                
            };
            Ok(::mudu_contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_command()  -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mudu_contract::tuple::datum_desc::DatumDesc::new(
                    "message".to_string(),
                    
                    ::mudu_type::dat_type::DatType::new_no_param(::mudu_type::dat_type_id::DatTypeID::Binary)
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_command() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mudu_contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    ::mudu_type::dat_type::DatType::new_no_param(::mudu_type::dat_type_id::DatTypeID::Binary)
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_command()  -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "game_backend".to_string(),
                "command".to_string(),
                mudu_argv_desc_command().clone(),
                mudu_result_desc_command().clone(),
                false
            )
        })
}

mod mod_command {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-command;
            world mudu-app-mp2-command {
                export mp2-command: func(param:list<u8>) -> list<u8>;
            }
        "##,
        
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestCommand {}

    impl Guest for GuestCommand {
         fn mp2_command(param:Vec<u8>) -> Vec<u8> {
            super::mp2_command(param)
        }
    }

    export!(GuestCommand);
}