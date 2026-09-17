//! Unit tests for the generated (WebAssembly) game-backend bindings.

use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::contract::procedure::procedure_param::ProcedureParam;
use mududb::types::data_value::DataValue;

#[test]
fn generated_command_wrapper_roundtrip() -> RS<()> {
    use crate::generated::procedure::mudu_inner_p2_command;

    let xid: OID = 9;
    let msg = vec![10, 20, 30];
    let param = ProcedureParam::new(xid, 0, vec![DataValue::from_binary(msg.clone())]);

    let result = mudu_inner_p2_command(param)?;
    let values = result.into();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].expect_binary(), &msg);
    Ok(())
}

#[test]
fn generated_event_wrapper_roundtrip() -> RS<()> {
    use crate::generated::procedure::mudu_inner_p2_event;

    let xid: OID = 10;
    let param = ProcedureParam::new(xid, 0, vec![]);

    let result = mudu_inner_p2_event(param)?;
    let values = result.into();
    assert_eq!(values.len(), 1);
    assert!(values[0].expect_binary().is_empty());
    Ok(())
}

#[test]
fn generated_command_describes_arguments_and_result() {
    use crate::generated::procedure::{
        mudu_argv_desc_command, mudu_proc_desc_command, mudu_result_desc_command,
    };
    use mududb::types::type_family::TypeFamily;

    let argv = mudu_argv_desc_command();
    assert_eq!(argv.fields().len(), 1);
    assert_eq!(argv.fields()[0].name(), "message");
    assert_eq!(
        argv.fields()[0].data_type().type_family(),
        TypeFamily::Binary
    );

    let result = mudu_result_desc_command();
    assert_eq!(result.fields().len(), 1);
    assert_eq!(result.fields()[0].name(), "0");
    assert_eq!(
        result.fields()[0].data_type().type_family(),
        TypeFamily::Binary
    );

    let proc = mudu_proc_desc_command();
    assert_eq!(proc.module_name(), "game_backend");
    assert_eq!(proc.proc_name(), "command");
}

#[test]
fn generated_event_describes_arguments_and_result() {
    use crate::generated::procedure::{
        mudu_argv_desc_event, mudu_proc_desc_event, mudu_result_desc_event,
    };
    use mududb::types::type_family::TypeFamily;

    let argv = mudu_argv_desc_event();
    assert!(argv.fields().is_empty());

    let result = mudu_result_desc_event();
    assert_eq!(result.fields().len(), 1);
    assert_eq!(result.fields()[0].name(), "0");
    assert_eq!(
        result.fields()[0].data_type().type_family(),
        TypeFamily::Binary
    );

    let proc = mudu_proc_desc_event();
    assert_eq!(proc.module_name(), "game_backend");
    assert_eq!(proc.proc_name(), "event");
}
