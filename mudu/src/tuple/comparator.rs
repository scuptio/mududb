use std::cmp::Ordering;
use std::fmt::Debug;
use std::hash::Hasher;

use crate::common::result::RS;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::param_obj::ParamObj;
use crate::error::ec::EC;
use crate::m_error;
use crate::tuple::dat_internal::DatInternal;
use crate::tuple::read_datum::{read_fixed_len_value, read_var_len_value};
use crate::tuple::tuple_binary_desc::TupleBinaryDesc;

pub fn tuple_compare(desc: &TupleBinaryDesc, tuple1: &[u8], tuple2: &[u8]) -> RS<Ordering> {
    _iter_value(
        desc,
        tuple1,
        tuple2,
        &_compare_binary_ordering,
        &_need_return_ordering,
        Ordering::Equal,
    )
}

pub fn tuple_equal(desc: &TupleBinaryDesc, tuple1: &[u8], tuple2: &[u8]) -> RS<bool> {
    _iter_value(
        desc,
        tuple1,
        tuple2,
        &_compare_binary_equal,
        &_need_return_equal,
        true,
    )
}

pub fn tuple_hash_finish(desc: &TupleBinaryDesc, tuple: &[u8], hasher: &mut dyn Hasher) -> RS<u64> {
    _tuple_hash(desc, tuple, hasher)?;
    let hash_value = hasher.finish();
    Ok(hash_value)
}

pub fn tuple_hash(desc: &TupleBinaryDesc, tuple: &[u8], hasher: &mut dyn Hasher) -> RS<()> {
    _tuple_hash(desc, tuple, hasher)?;
    Ok(())
}

fn _tuple_hash(desc: &TupleBinaryDesc, tuple: &[u8], hasher: &mut dyn Hasher) -> RS<()> {
    for fd in desc.fixed_len_field_desc() {
        let value = read_fixed_len_value(fd.slot().offset(), fd.slot().length(), tuple)?;
        _hash_binary(fd.data_type(), fd.type_param(), value, hasher)?;
    }

    for fd in desc.var_len_field_desc() {
        let value = read_var_len_value(fd.slot().offset(), tuple)?;
        _hash_binary(fd.data_type(), fd.type_param(), value, hasher)?;
    }
    Ok(())
}

fn _hash_binary(id: DatTypeID, p: &ParamObj, val: &[u8], hasher: &mut dyn Hasher) -> RS<()> {
    let recv = id.fn_recv();
    let v_internal =
        recv(val, p).map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?;
    if let Some(h) = id.fn_hash() {
        h(&v_internal, hasher).map_err(|e| m_error!(EC::CompareErr, "hash binary error", e))
    } else {
        Err(m_error!(EC::TupleErr))
    }
}

fn _compare_binary<
    F: Fn(&DatTypeID, &DatInternal, &DatInternal) -> RS<R> + 'static,
    R: Debug + Copy + Clone + 'static,
>(
    id: DatTypeID,
    param: &ParamObj,
    value1: &[u8],
    value2: &[u8],
    compare: &F,
) -> RS<R> {
    let recv = id.fn_recv();
    let r1 = recv(value1, param);
    let r2 = recv(value2, param);
    match (r1, r2) {
        (Ok(v1), Ok(v2)) => compare(&id, &v1, &v2),
        _ => Err(m_error!(EC::TupleErr)),
    }
}

fn _compare_binary_equal(
    data_type: &DatTypeID,
    value1: &DatInternal,
    value2: &DatInternal,
) -> RS<bool> {
    let opt_equal = data_type.fn_equal();
    let f = match opt_equal {
        None => return Err(m_error!(EC::FunctionNotImplemented)),
        Some(f) => f,
    };
    let r = f(value1, value2);
    match r {
        Ok(is_equal) => Ok(is_equal),
        Err(_e) => Err(m_error!(EC::CompareErr, "compare binary equal error", _e)),
    }
}

fn _compare_binary_ordering(
    data_type: &DatTypeID,
    value1: &DatInternal,
    value2: &DatInternal,
) -> RS<Ordering> {
    let opt_order = data_type.fn_order();
    let f = match opt_order {
        None => return Err(m_error!(EC::FunctionNotImplemented)),
        Some(f) => f,
    };
    let r = f(value1, value2);
    match r {
        Ok(ordering) => Ok(ordering),
        Err(_e) => Err(m_error!(EC::CompareErr, "compare binary order error", _e)),
    }
}

fn _need_return_ordering(ord: Ordering) -> bool {
    ord.is_ne()
}

fn _need_return_equal(equal: bool) -> bool {
    !equal
}

fn _compare_opt_binary<
    F: Fn(&DatTypeID, &DatInternal, &DatInternal) -> RS<R> + 'static,
    R: Debug + Copy + Clone + 'static,
>(
    id: DatTypeID,
    param: &ParamObj,
    value1: &[u8],
    value2: &[u8],
    compare: &F,
) -> RS<R> {
    let r = _compare_binary(id, param, value1, value2, compare)?;
    Ok(r)
}

fn _iter_value<
    F: Fn(&DatTypeID, &DatInternal, &DatInternal) -> RS<R> + 'static,
    R: Debug + Copy + Clone + 'static,
    T: Fn(R) -> bool + 'static,
>(
    desc: &TupleBinaryDesc,
    tuple1: &[u8],
    tuple2: &[u8],
    compare: &F,
    need_return: &T,
    ret: R,
) -> RS<R> {
    for fd in desc.fixed_len_field_desc() {
        let opt1 = read_fixed_len_value(fd.slot().offset(), fd.slot().length(), tuple1)?;
        let opt2 = read_fixed_len_value(fd.slot().offset(), fd.slot().length(), tuple2)?;
        let ord = _compare_opt_binary(fd.data_type(), fd.type_param(), opt1, opt2, compare)?;
        if need_return(ord) {
            return Ok(ord);
        }
    }

    for fd in desc.var_len_field_desc() {
        let opt1 = read_var_len_value(fd.slot().offset(), tuple1)?;
        let opt2 = read_var_len_value(fd.slot().offset(), tuple2)?;
        let ord = _compare_opt_binary(fd.data_type(), fd.type_param(), opt1, opt2, compare)?;
        if need_return(ord) {
            return Ok(ord);
        }
    }
    Ok(ret)
}
