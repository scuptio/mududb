//! `tuple::comparator` module.
#![allow(missing_docs)]

use std::cmp::Ordering;
use std::fmt::Debug;
use std::hash::Hasher;

use crate::tuple::read_datum::{read_fixed_len_value, read_var_len_value};
use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::type_family::TypeFamily;

#[derive(Clone, Copy)]
pub struct TupleComparator {
    pub compare: fn(&[u8], &[u8], &TupleBinaryDesc) -> RS<Ordering>,
    pub equal: fn(&[u8], &[u8], &TupleBinaryDesc) -> RS<bool>,
    pub hash_cal_one: fn(&[u8], &TupleBinaryDesc, &mut dyn Hasher) -> RS<()>,
    pub hash_cal_finish: fn(&[u8], &TupleBinaryDesc, &mut dyn Hasher) -> RS<u64>,
}

impl TupleComparator {
    pub fn new() -> Self {
        Self {
            compare: tuple_compare_adapter,
            equal: tuple_equal_adapter,
            hash_cal_one: tuple_hash_adapter,
            hash_cal_finish: tuple_hash_finish_adapter,
        }
    }
}

impl Default for TupleComparator {
    fn default() -> Self {
        Self::new()
    }
}

fn tuple_compare_adapter(tuple1: &[u8], tuple2: &[u8], desc: &TupleBinaryDesc) -> RS<Ordering> {
    tuple_compare(desc, tuple1, tuple2)
}

fn tuple_equal_adapter(tuple1: &[u8], tuple2: &[u8], desc: &TupleBinaryDesc) -> RS<bool> {
    tuple_equal(desc, tuple1, tuple2)
}

fn tuple_hash_adapter(tuple: &[u8], desc: &TupleBinaryDesc, hasher: &mut dyn Hasher) -> RS<()> {
    tuple_hash(desc, tuple, hasher)
}

fn tuple_hash_finish_adapter(
    tuple: &[u8],
    desc: &TupleBinaryDesc,
    hasher: &mut dyn Hasher,
) -> RS<u64> {
    tuple_hash_finish(desc, tuple, hasher)
}

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
        _hash_binary(fd.data_type(), fd.type_obj(), value, hasher)?;
    }

    for fd in desc.var_len_field_desc() {
        let value = read_var_len_value(fd.slot().offset(), tuple)?;
        _hash_binary(fd.data_type(), fd.type_obj(), value, hasher)?;
    }
    Ok(())
}

fn _hash_binary(id: TypeFamily, p: &DataType, val: &[u8], hasher: &mut dyn Hasher) -> RS<()> {
    let recv = id.fn_recv();
    let (v_internal, _size) = recv(val, p).map_err(|e| {
        mudu_error!(
            ErrorCode::TypeConversionFailed,
            "convert data format error",
            e
        )
    })?;
    if let Some(h) = id.fn_hash() {
        h(&v_internal, hasher)
            .map_err(|e| mudu_error!(ErrorCode::HashFailed, "hash binary error", e))
    } else {
        Err(mudu_error!(ErrorCode::InvalidTuple))
    }
}

fn _compare_binary<
    F: Fn(&TypeFamily, &DataValue, &DataValue) -> RS<R> + 'static,
    R: Debug + Copy + Clone + 'static,
>(
    id: TypeFamily,
    param: &DataType,
    value1: &[u8],
    value2: &[u8],
    compare: &F,
) -> RS<R> {
    let recv = id.fn_recv();
    let r1 = recv(value1, param);
    let r2 = recv(value2, param);
    match (r1, r2) {
        (Ok((v1, _)), Ok((v2, _))) => compare(&id, &v1, &v2),
        _ => Err(mudu_error!(ErrorCode::InvalidTuple)),
    }
}

fn _compare_binary_equal(
    data_type: &TypeFamily,
    value1: &DataValue,
    value2: &DataValue,
) -> RS<bool> {
    let opt_equal = data_type.fn_equal();
    let f = match opt_equal {
        None => return Err(mudu_error!(ErrorCode::UnsupportedOperation)),
        Some(f) => f,
    };
    f(value1, value2).map_err(|_e| {
        mudu_error!(
            ErrorCode::ComparisonFailed,
            "compare binary equal error",
            _e
        )
    })
}

fn _compare_binary_ordering(
    data_type: &TypeFamily,
    value1: &DataValue,
    value2: &DataValue,
) -> RS<Ordering> {
    let opt_order = data_type.fn_order();
    let f = match opt_order {
        None => return Err(mudu_error!(ErrorCode::UnsupportedOperation)),
        Some(f) => f,
    };
    f(value1, value2).map_err(|_e| {
        mudu_error!(
            ErrorCode::ComparisonFailed,
            "compare binary order error",
            _e
        )
    })
}

fn _need_return_ordering(ord: Ordering) -> bool {
    ord.is_ne()
}

fn _need_return_equal(equal: bool) -> bool {
    !equal
}

fn _compare_opt_binary<
    F: Fn(&TypeFamily, &DataValue, &DataValue) -> RS<R> + 'static,
    R: Debug + Copy + Clone + 'static,
>(
    id: TypeFamily,
    param: &DataType,
    value1: &[u8],
    value2: &[u8],
    compare: &F,
) -> RS<R> {
    let r = _compare_binary(id, param, value1, value2, compare)?;
    Ok(r)
}

fn _iter_value<
    F: Fn(&TypeFamily, &DataValue, &DataValue) -> RS<R> + 'static,
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
        let ord = _compare_opt_binary(fd.data_type(), fd.type_obj(), opt1, opt2, compare)?;
        if need_return(ord) {
            return Ok(ord);
        }
    }

    for fd in desc.var_len_field_desc() {
        let opt1 = read_var_len_value(fd.slot().offset(), tuple1)?;
        let opt2 = read_var_len_value(fd.slot().offset(), tuple2)?;
        let ord = _compare_opt_binary(fd.data_type(), fd.type_obj(), opt1, opt2, compare)?;
        if need_return(ord) {
            return Ok(ord);
        }
    }
    Ok(ret)
}
