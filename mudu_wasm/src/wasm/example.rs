use mudu::common::result::RS;
use mudu::common::serde_utils::{deserialize_sized_from, serialize_sized_to};
use mudu::common::xid::XID;
use mudu::error::error::ER;
use mudu::procedure::proc_param::ProcParam;
use mudu::tuple::dat_internal::DatInternal;
use mudu::tuple::tuple_item_desc::TupleItemDesc;
use std::slice;

pub extern "C" fn example_run(p1_ptr: *const u8, p1_len: usize, p2_ptr: *mut u8, p2_len: usize) -> i32 {
    let r = _example_run(p1_ptr, p1_len, p2_ptr, p2_len);
    match r {
        Ok(()) => 0,
        Err(_e) => -1
    }
}

fn _example_run(p1_ptr: *const u8, p1_len: usize, p2_ptr: *mut u8, p2_len: usize) -> RS<()> {
    let param: ProcParam = unsafe {
        let slice = slice::from_raw_parts(p1_ptr, p1_len);
        let (param, _size) = deserialize_sized_from::<ProcParam>(slice)?;
        param
    };
    let param_desc = TupleItemDesc::new(vec![]);
    let desc0 = &param_desc.desc()[0];
    let p1: DatInternal = desc0.dat_type_id()
        .fn_recv()(
        &param.param_vec()[0],
        desc0.type_declare().param(),
    ).map_err(|e| {
        ER::MuduError(e.to_string())
    })?;
    let desc1 = &param_desc.desc()[1];
    let p2: DatInternal = desc1.dat_type_id()
        .fn_recv()(
        &param.param_vec()[2],
        desc0.type_declare().param(),
    ).map_err(|e| {
        ER::MuduError(e.to_string())
    })?;

    let result = _example(param.xid(), p1.to_i32(), p2.to_i32());
    let out_buf = unsafe {
        let slice = slice::from_raw_parts_mut(p2_ptr, p2_len);
        slice
    };

    serialize_sized_to(&result, out_buf)?;
    Ok(())
}

fn _example(_xid: XID, p1: i32, p2: i32) -> RS<(i32, i32)> {
    Ok((p2, p1))
}

