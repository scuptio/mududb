//! Manual-timing benchmarks for the MSSP syscall payload codec.
//!
//! Follows the repo bench convention (see
//! `crates/common/sql_parser/tests/parse_bench.rs`): plain `#[test]` functions
//! with manual timing via `mudu_sys::time::instant_now()`, no criterion.
//!
//! Every workload measures the full MSSP frame path (16-byte header +
//! MessagePack body, kind validation on decode) against a raw `mp_wire`
//! baseline that encodes/decodes the byte-identical body value with no
//! header and no routing. The framing overhead is reported as a percentage,
//! which is the machine-independent figure; absolute µs/op numbers are
//! indicative only.
//!
//! Run with:
//!
//! ```sh
//! cargo test -p mudu_binding --test syscall_payload_bench -- --nocapture
//! ```

use std::hint::black_box;

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::codec::syscall_payload::{
    HEADER_LEN, decode_delete_request, decode_delete_result, decode_fs_open_request,
    decode_fs_readdir_result, decode_get_request, decode_get_result, decode_put_request,
    decode_put_result, decode_query_request, decode_query_result, decode_range_request,
    decode_range_result, encode_delete_request, encode_delete_result, encode_fs_open_request,
    encode_fs_readdir_result, encode_get_request, encode_get_result, encode_put_request,
    encode_put_result, encode_query_request, encode_query_result, encode_range_request,
    encode_range_result,
};
use mudu_binding::universal::mp_wire::{
    ToValue, Value, decode_value, encode_value, to_array, to_tuple2,
};
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_data_value::UniDataValue;
use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
use mudu_binding::universal::uni_fs_open_argv::UniFsOpenArgv;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_binding::universal::uni_query_argv::UniQueryArgv;
use mudu_binding::universal::uni_query_result::UniQueryResult;
use mudu_binding::universal::uni_record_type::{UniRecordField, UniRecordType};
use mudu_binding::universal::uni_result_set::UniResultSet;
use mudu_binding::universal::uni_scalar::UniScalar;
use mudu_binding::universal::uni_scalar_value::UniScalarValue;
use mudu_binding::universal::uni_sql_param::UniSqlParam;
use mudu_binding::universal::uni_sql_stmt::UniSqlStmt;
use mudu_binding::universal::uni_tuple_row::UniTupleRow;

// ---------------------------------------------------------------------------
// Sample data
// ---------------------------------------------------------------------------

fn sample_oid() -> UniOid {
    UniOid {
        h: 0x0102_0304_0506_0708,
        l: 0x1112_1314_1516_1718,
    }
}

fn sample_params(n: usize) -> Vec<UniDataValue> {
    (0..n)
        .map(|i| match i % 4 {
            0 => UniDataValue::from_scalar(UniScalarValue::from_i64(i as i64)),
            1 => UniDataValue::from_scalar(UniScalarValue::from_string(format!("param-{i}"))),
            2 => UniDataValue::from_scalar(UniScalarValue::from_f64(i as f64 + 0.5)),
            _ => UniDataValue::from_scalar(UniScalarValue::from_bool(i % 2 == 0)),
        })
        .collect()
}

fn query_argv(sql: String, n_params: usize) -> UniQueryArgv {
    UniQueryArgv {
        oid: sample_oid(),
        query: UniSqlStmt { sql_string: sql },
        param_list: UniSqlParam {
            params: sample_params(n_params),
            param_names: None,
        },
        param_desc: None,
    }
}

fn long_sql() -> String {
    let mut sql = String::from("select ");
    while sql.len() < 4096 {
        sql.push_str("col_abcdefghijklmnopqrstuvwxyz_0123, ");
    }
    sql.push_str("id from t where id = ?");
    sql
}

fn field(name: &str, scalar: UniScalar) -> UniRecordField {
    UniRecordField {
        field_name: name.to_string(),
        field_type: UniDataType::from_scalar(scalar),
        field_attrs: vec![],
    }
}

fn query_result(n_rows: usize) -> UniQueryResult {
    UniQueryResult {
        tuple_desc: UniRecordType {
            record_name: "row".to_string(),
            record_fields: vec![
                field("id", UniScalar::I64),
                field("name", UniScalar::String),
                field("data", UniScalar::Blob),
                field("score", UniScalar::F64),
                field("active", UniScalar::Bool),
                // The universal value model has no dedicated null variant;
                // this column stands in for a nullable column and carries the
                // default placeholder value.
                UniRecordField {
                    field_name: "misc".to_string(),
                    field_type: UniDataType::from_option(Box::new(UniDataType::from_scalar(
                        UniScalar::String,
                    ))),
                    field_attrs: vec![],
                },
            ],
        },
        result_set: UniResultSet {
            eof: true,
            row_set: (0..n_rows)
                .map(|i| UniTupleRow {
                    fields: vec![
                        UniDataValue::from_scalar(UniScalarValue::from_i64(i as i64)),
                        UniDataValue::from_scalar(UniScalarValue::from_string(format!("name-{i}"))),
                        UniDataValue::from_scalar(UniScalarValue::from_blob(vec![
                            (i % 251) as u8;
                            16
                        ])),
                        UniDataValue::from_scalar(UniScalarValue::from_f64(i as f64 + 0.25)),
                        UniDataValue::from_scalar(UniScalarValue::from_bool(i % 2 == 0)),
                        UniDataValue::default(),
                    ],
                })
                .collect(),
            cursor: vec![],
        },
    }
}

fn kv_key(i: usize) -> Vec<u8> {
    format!("key-{i:08}").into_bytes()
}

fn kv_value(i: usize) -> Vec<u8> {
    format!("value-{i:08}-0123456789abcdef0123456789abcdef0123456789abcdef").into_bytes()
}

fn readdir_entries(n: usize) -> Vec<UniFsDirent> {
    (0..n)
        .map(|i| UniFsDirent {
            name: format!("entry-{i:04}.dat"),
            is_dir: i % 7 == 0,
            length: (i as u64) * 97,
        })
        .collect()
}

fn fs_open_argv() -> UniFsOpenArgv {
    UniFsOpenArgv {
        session: sample_oid(),
        oid: sample_oid(),
        path: "/data/segment/0000000001.dat".to_string(),
        flags: 3,
    }
}

// ---------------------------------------------------------------------------
// Raw mp_wire baseline helpers
//
// The baseline body value is byte-identical to the MSSP body; the baseline
// therefore isolates exactly the framing + routing + typed-conversion cost.
// ---------------------------------------------------------------------------

/// Request body for a single-record argument: `{1: record}`.
fn one_arg_body<T: ToValue>(argv: &T) -> Value {
    Value::Map(vec![(Value::from(1u32), argv.to_value())])
}

/// KV request body: `{1: oid, 2: bin}` (and `{3: bin}` for put).
fn kv_req_body(oid: &UniOid, blobs: &[&[u8]]) -> Value {
    let mut pairs = vec![(Value::from(1u32), oid.to_value())];
    for (i, blob) in blobs.iter().enumerate() {
        pairs.push((Value::from((i + 2) as u32), Value::Bin(blob.to_vec())));
    }
    Value::Map(pairs)
}

/// Result body `[0u8, value]`.
fn ok_body(value: Value) -> Value {
    Value::Array(vec![Value::from(0u8), value])
}

/// Unit result body `[0u8, 0u8]`.
fn unit_body() -> Value {
    Value::Array(vec![Value::from(0u8), Value::from(0u8)])
}

// ---------------------------------------------------------------------------
// Timing and reporting
// ---------------------------------------------------------------------------

/// Returns the mean µs per operation after `warmup` untimed iterations.
fn time_us_per_op<T>(iters: usize, warmup: usize, f: &mut dyn FnMut() -> T) -> f64 {
    for _ in 0..warmup {
        black_box(f());
    }
    let start = mudu_sys::time::instant_now();
    for _ in 0..iters {
        black_box(f());
    }
    start.elapsed().as_secs_f64() * 1e6 / iters as f64
}

fn overhead_pct(mssp: f64, raw: f64) -> f64 {
    (mssp - raw) / raw * 100.0
}

#[allow(clippy::too_many_arguments)]
fn report<A, B, C, D>(
    name: &str,
    iters: usize,
    warmup: usize,
    frame_len: usize,
    mut mssp_enc: impl FnMut() -> A,
    mut mssp_dec: impl FnMut() -> B,
    mut raw_enc: impl FnMut() -> C,
    mut raw_dec: impl FnMut() -> D,
) {
    let enc = time_us_per_op(iters, warmup, &mut mssp_enc);
    let dec = time_us_per_op(iters, warmup, &mut mssp_dec);
    let raw_enc = time_us_per_op(iters, warmup, &mut raw_enc);
    let raw_dec = time_us_per_op(iters, warmup, &mut raw_dec);
    println!(
        "{name:<24} iters={iters:<7} frame={frame_len:>7}B body={:>7}B \
         | mssp enc {enc:>8.2} us dec {dec:>8.2} us \
         | raw enc {raw_enc:>8.2} us dec {raw_dec:>8.2} us \
         | framing enc {:>+7.1}% dec {:>+7.1}%",
        frame_len - HEADER_LEN,
        overhead_pct(enc, raw_enc),
        overhead_pct(dec, raw_dec),
    );
}

// ---------------------------------------------------------------------------
// Workloads
// ---------------------------------------------------------------------------

/// Query request frames: short SQL with 10 params, ~4KB SQL with 100 params.
#[cfg_attr(miri, ignore)]
#[test]
fn bench_query_request() {
    for (name, argv, iters) in [
        (
            "query_req_short",
            query_argv("select * from t where id = ?".to_string(), 10),
            20_000usize,
        ),
        ("query_req_long4k", query_argv(long_sql(), 100), 2_000usize),
    ] {
        let frame = encode_query_request(&argv);
        // Sanity: MSSP round-trip, then raw body is byte-identical to the
        // MSSP body.
        let decoded = decode_query_request(&frame).unwrap();
        assert_eq!(encode_query_request(&decoded), frame);
        let body = one_arg_body(&argv);
        let raw_bytes = encode_value(&body);
        assert_eq!(raw_bytes, &frame[HEADER_LEN..]);
        assert!(decode_value(&raw_bytes).is_ok());

        let raw_body = &frame[HEADER_LEN..];
        report(
            name,
            iters,
            iters / 10,
            frame.len(),
            || black_box(encode_query_request(black_box(&argv))),
            || black_box(decode_query_request(black_box(&frame)).unwrap()),
            || black_box(encode_value(black_box(&body))),
            || black_box(decode_value(black_box(raw_body)).unwrap()),
        );
    }
}

/// Query result frames: 1, 100 and 10_000 rows of mixed scalar columns.
#[cfg_attr(miri, ignore)]
#[test]
fn bench_query_result() {
    for (name, rows, iters) in [
        ("query_result_1row", 1usize, 20_000usize),
        ("query_result_100rows", 100, 1_000),
        ("query_result_10krows", 10_000, 100),
    ] {
        let result: RS<UniQueryResult> = Ok(query_result(rows));
        let frame = encode_query_result(&result);
        let decoded = decode_query_result(&frame).unwrap();
        assert_eq!(encode_query_result(&Ok(decoded)), frame);
        let value = result.as_ref().unwrap();
        let body = ok_body(value.to_value());
        let raw_bytes = encode_value(&body);
        assert_eq!(raw_bytes, &frame[HEADER_LEN..]);
        assert!(decode_value(&raw_bytes).is_ok());

        let raw_body = &frame[HEADER_LEN..];
        report(
            name,
            iters,
            (iters / 10).max(10),
            frame.len(),
            || black_box(encode_query_result(black_box(&result))),
            || black_box(decode_query_result(black_box(&frame)).unwrap()),
            || black_box(encode_value(black_box(&body))),
            || black_box(decode_value(black_box(raw_body)).unwrap()),
        );
    }
}

/// KV frames: get/put/delete/range requests, ok results, 100-pair range result.
#[cfg_attr(miri, ignore)]
#[test]
fn bench_kv() {
    let oid = sample_oid();
    let key = kv_key(1);
    let value = kv_value(1);
    let pairs: Vec<(Vec<u8>, Vec<u8>)> = (0..100).map(|i| (kv_key(i), kv_value(i))).collect();
    let iters = 20_000usize;

    // get request: body {1: oid, 2: key}
    let frame = encode_get_request(oid.clone(), &key);
    let (d_oid, d_key) = decode_get_request(&frame).unwrap();
    assert_eq!((d_oid.h, d_oid.l, d_key), (oid.h, oid.l, key.clone()));
    let body = kv_req_body(&oid, &[&key]);
    assert_eq!(encode_value(&body), &frame[HEADER_LEN..]);
    let raw_body = &frame[HEADER_LEN..];
    report(
        "kv_get_req",
        iters,
        iters / 10,
        frame.len(),
        || black_box(encode_get_request(black_box(oid.clone()), black_box(&key))),
        || black_box(decode_get_request(black_box(&frame)).unwrap()),
        || black_box(encode_value(black_box(&body))),
        || black_box(decode_value(black_box(raw_body)).unwrap()),
    );

    // get result: body [0, value]
    let get_result: RS<Option<Vec<u8>>> = Ok(Some(value.clone()));
    let frame = encode_get_result(&get_result);
    assert_eq!(decode_get_result(&frame).unwrap(), Some(value.clone()));
    let body = ok_body(Value::Bin(value.clone()));
    assert_eq!(encode_value(&body), &frame[HEADER_LEN..]);
    let raw_body = &frame[HEADER_LEN..];
    report(
        "kv_get_result",
        iters,
        iters / 10,
        frame.len(),
        || black_box(encode_get_result(black_box(&get_result))),
        || black_box(decode_get_result(black_box(&frame)).unwrap()),
        || black_box(encode_value(black_box(&body))),
        || black_box(decode_value(black_box(raw_body)).unwrap()),
    );

    // put request: body {1: oid, 2: key, 3: value}
    let frame = encode_put_request(oid.clone(), &key, &value);
    let (d_oid, d_key, d_value) = decode_put_request(&frame).unwrap();
    assert_eq!(
        (d_oid.h, d_oid.l, d_key, d_value),
        (oid.h, oid.l, key.clone(), value.clone())
    );
    let body = kv_req_body(&oid, &[&key, &value]);
    assert_eq!(encode_value(&body), &frame[HEADER_LEN..]);
    let raw_body = &frame[HEADER_LEN..];
    report(
        "kv_put_req",
        iters,
        iters / 10,
        frame.len(),
        || {
            black_box(encode_put_request(
                black_box(oid.clone()),
                black_box(&key),
                black_box(&value),
            ))
        },
        || black_box(decode_put_request(black_box(&frame)).unwrap()),
        || black_box(encode_value(black_box(&body))),
        || black_box(decode_value(black_box(raw_body)).unwrap()),
    );

    // put result: body [0, 0]
    let unit: RS<()> = Ok(());
    let frame = encode_put_result(&unit);
    decode_put_result(&frame).unwrap();
    let body = unit_body();
    assert_eq!(encode_value(&body), &frame[HEADER_LEN..]);
    let raw_body = &frame[HEADER_LEN..];
    report(
        "kv_put_result",
        iters,
        iters / 10,
        frame.len(),
        || black_box(encode_put_result(black_box(&unit))),
        || decode_put_result(black_box(&frame)).unwrap(),
        || black_box(encode_value(black_box(&body))),
        || black_box(decode_value(black_box(raw_body)).unwrap()),
    );

    // delete request: body {1: oid, 2: key}
    let frame = encode_delete_request(oid.clone(), &key);
    let (d_oid, d_key) = decode_delete_request(&frame).unwrap();
    assert_eq!((d_oid.h, d_oid.l, d_key), (oid.h, oid.l, key.clone()));
    let body = kv_req_body(&oid, &[&key]);
    assert_eq!(encode_value(&body), &frame[HEADER_LEN..]);
    let raw_body = &frame[HEADER_LEN..];
    report(
        "kv_delete_req",
        iters,
        iters / 10,
        frame.len(),
        || {
            black_box(encode_delete_request(
                black_box(oid.clone()),
                black_box(&key),
            ))
        },
        || black_box(decode_delete_request(black_box(&frame)).unwrap()),
        || black_box(encode_value(black_box(&body))),
        || black_box(decode_value(black_box(raw_body)).unwrap()),
    );

    // delete result: body [0, 0]
    let frame = encode_delete_result(&unit);
    decode_delete_result(&frame).unwrap();
    let body = unit_body();
    assert_eq!(encode_value(&body), &frame[HEADER_LEN..]);
    let raw_body = &frame[HEADER_LEN..];
    report(
        "kv_delete_result",
        iters,
        iters / 10,
        frame.len(),
        || black_box(encode_delete_result(black_box(&unit))),
        || decode_delete_result(black_box(&frame)).unwrap(),
        || black_box(encode_value(black_box(&body))),
        || black_box(decode_value(black_box(raw_body)).unwrap()),
    );

    // range request: body {1: oid, 2: start, 3: end}
    let end = kv_key(999);
    let frame = encode_range_request(oid.clone(), &key, &end);
    let (d_oid, d_start, d_end) = decode_range_request(&frame).unwrap();
    assert_eq!(
        (d_oid.h, d_oid.l, d_start, d_end),
        (oid.h, oid.l, key.clone(), end.clone())
    );
    let body = kv_req_body(&oid, &[&key, &end]);
    assert_eq!(encode_value(&body), &frame[HEADER_LEN..]);
    let raw_body = &frame[HEADER_LEN..];
    report(
        "kv_range_req",
        iters,
        iters / 10,
        frame.len(),
        || {
            black_box(encode_range_request(
                black_box(oid.clone()),
                black_box(&key),
                black_box(&end),
            ))
        },
        || black_box(decode_range_request(black_box(&frame)).unwrap()),
        || black_box(encode_value(black_box(&body))),
        || black_box(decode_value(black_box(raw_body)).unwrap()),
    );

    // range result: body [0, [[key, value], ...]] with 100 pairs
    let range_result: RS<Vec<(Vec<u8>, Vec<u8>)>> = Ok(pairs.clone());
    let frame = encode_range_result(&range_result);
    assert_eq!(decode_range_result(&frame).unwrap(), pairs);
    let body = ok_body(to_array(&pairs, |pair| {
        to_tuple2(pair, |k| Value::Bin(k.clone()), |v| Value::Bin(v.clone()))
    }));
    assert_eq!(encode_value(&body), &frame[HEADER_LEN..]);
    let raw_body = &frame[HEADER_LEN..];
    report(
        "kv_range_result_100",
        iters,
        iters / 10,
        frame.len(),
        || black_box(encode_range_result(black_box(&range_result))),
        || black_box(decode_range_result(black_box(&frame)).unwrap()),
        || black_box(encode_value(black_box(&body))),
        || black_box(decode_value(black_box(raw_body)).unwrap()),
    );
}

/// fs-open request and fs-readdir result (50 entries) frames.
#[cfg_attr(miri, ignore)]
#[test]
fn bench_fs() {
    let iters = 20_000usize;

    let argv = fs_open_argv();
    let frame = encode_fs_open_request(&argv);
    let decoded = decode_fs_open_request(&frame).unwrap();
    assert_eq!(encode_fs_open_request(&decoded), frame);
    let body = one_arg_body(&argv);
    assert_eq!(encode_value(&body), &frame[HEADER_LEN..]);
    let raw_body = &frame[HEADER_LEN..];
    report(
        "fs_open_req",
        iters,
        iters / 10,
        frame.len(),
        || black_box(encode_fs_open_request(black_box(&argv))),
        || black_box(decode_fs_open_request(black_box(&frame)).unwrap()),
        || black_box(encode_value(black_box(&body))),
        || black_box(decode_value(black_box(raw_body)).unwrap()),
    );

    let entries = readdir_entries(50);
    let readdir_result: RS<Vec<UniFsDirent>> = Ok(entries);
    let frame = encode_fs_readdir_result(&readdir_result);
    let decoded = decode_fs_readdir_result(&frame).unwrap();
    assert_eq!(encode_fs_readdir_result(&Ok(decoded)), frame);
    let entries = readdir_result.as_ref().unwrap();
    let body = ok_body(to_array(entries, |entry| entry.to_value()));
    assert_eq!(encode_value(&body), &frame[HEADER_LEN..]);
    let raw_body = &frame[HEADER_LEN..];
    report(
        "fs_readdir_result_50",
        iters,
        iters / 10,
        frame.len(),
        || black_box(encode_fs_readdir_result(black_box(&readdir_result))),
        || black_box(decode_fs_readdir_result(black_box(&frame)).unwrap()),
        || black_box(encode_value(black_box(&body))),
        || black_box(decode_value(black_box(raw_body)).unwrap()),
    );
}

/// UniError error response frame (get result carrying a `NotFound` error).
#[cfg_attr(miri, ignore)]
#[test]
fn bench_error_response() {
    let iters = 20_000usize;
    let err_result: RS<Option<Vec<u8>>> = Err(mudu_error!(ErrorCode::NotFound, "no such key"));
    let frame = encode_get_result(&err_result);
    let decoded = decode_get_result(&frame);
    assert_eq!(decoded.err().map(|e| e.ec()), Some(ErrorCode::NotFound));
    // The raw baseline reuses the body value already materialized in the
    // frame body.
    let body = decode_value(&frame[HEADER_LEN..]).unwrap();
    assert_eq!(body.as_array().unwrap()[0], Value::from(1u8));
    assert_eq!(encode_value(&body), &frame[HEADER_LEN..]);
    let raw_body = &frame[HEADER_LEN..];
    report(
        "error_result",
        iters,
        iters / 10,
        frame.len(),
        || black_box(encode_get_result(black_box(&err_result))),
        || black_box(decode_get_result(black_box(&frame)).err().unwrap()),
        || black_box(encode_value(black_box(&body))),
        || black_box(decode_value(black_box(raw_body)).unwrap()),
    );
}
