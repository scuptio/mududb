use super::syscall_v1::golden_syscall_error;
use super::*;

// ---------------------------------------------------------------------------
// SyscallPayload v1 (MSSP) all-kinds corpus
//
// `syscall_payload_v1_all.bin` packs 47 canonical frames in the same
// big-endian-u32 length-prefixed segment layout as `syscall_payload_v1.bin`:
// for each of the 23 message kinds (in `MessageKind` discriminant order)
// one request frame followed by one ok response frame, plus one trailing
// `get` UniError response frame (segment index 46).
//
// The corpus is the cross-language interop proof for the guest/host ABI:
// the standalone Rust SDK (`crates/sdk/mudu_api/rust`) and the C# SDK
// (`crates/sdk/mudu_api/csharp`) decode and re-encode these exact bytes in
// their own golden tests. The host codec here is the source of truth.
//
// `syscall_payload_v1_all.json` is the JSON sidecar of the corpus (same
// pattern as `mp_primitives_v1.json`): top-level
// format/reference/container/regenerate/conventions fields plus a `frames`
// array with one entry per segment carrying (index, message_kind,
// message_kind_name, direction, expect). The `expect` object describes the
// decoded semantics of the frame as JSON (u64/i64 as decimal strings, byte
// strings as lowercase hex, oids as {h, l} decimal strings, unit results as
// {"unit": true}), so the C# and AssemblyScript consumers can rebuild every
// expected value without a Rust toolchain. Frame bytes and sidecar
// expectations are derived from the SAME constructors below
// (`golden_syscall_all_cases`), so the two cannot drift apart.
//
// Assertion style: the corpus is pinned by per-frame semantic assertions and
// decode → re-encode → decode roundtrips, not by a whole-file byte anchor.
// Request frames additionally re-encode byte-exactly (pinning the canonical
// encoder); the trailing UniError frame stays field-level (a decoded error
// cannot re-encode byte-exactly because decoding assigns a fresh caller
// location).
//
// Documented deterministic inputs (reconstructed verbatim by the guests):
//
//   common:    oid N = UniOid { h: 0, l: N }; fds are u32 3; blobs are ASCII
//   1  query            oid 1, sql "select 1", no params
//                       ok: UniQueryResult default (record_name "", no
//                       fields, eof false, no rows, empty cursor)
//   2  command          oid 2, sql "update t set a = 1", no params
//                       ok: affected_rows 3
//   3  batch            oid 3, sql "insert into t values (1)", no params
//                       ok: affected_rows 2
//   4  open-session     worker_id oid 4
//                       ok: session oid 4 (host OID u128 = 4)
//   5  close-session    oid 5
//                       ok: unit [0, 0]
//   6  get              oid 6, key "k1"
//                       ok: Some value "v1"
//   7  put              oid 7, key "k1", value "v1"
//                       ok: unit
//   8  delete           oid 8, key "k1"
//                       ok: unit
//   9  range            oid 9, start "a", end "z"
//                       ok: [("a", "1"), ("b", "2")]
//   10 fs-open          session oid 10, oid 11, path "docs/a.txt", flags 2
//                       ok: fd 9
//   11 fs-close         fd 3
//                       ok: unit
//   12 fs-read          fd 3, len 4
//                       ok: data "hi"
//   13 fs-write         fd 3, data "hi"
//                       ok: written 2
//   14 fs-pread         fd 3, offset 8, len 4
//                       ok: data "hi"
//   15 fs-pwrite        fd 3, offset 8, data "hi"
//                       ok: unit
//   16 fs-lseek         fd 3, offset -2, whence 1
//                       ok: new cursor 6
//   17 fs-fstat         fd 3
//                       ok: stat { oid 5, generation 1, entry "",
//                       length 100, state 1 }
//   18 fs-stat          oid 18, path "a"
//                       ok: the same stat record
//   19 fs-fsync         fd 3
//                       ok: unit
//   20 fs-readdir       oid 20, path "d"
//                       ok: [{ "a.txt", false, 3 }, { "docs", true, 0 }]
//   21 relation-get     oid 21, table "t", key [(1, [01]), (2, [02 03])],
//                       select [3, 4]
//                       ok: row [Some [0A], None]
//   22 relation-update  oid 22, table "t", key [(1, [01])],
//                       values [(2, [0A])], deltas [(3, op 0 (add), [05])]
//                       ok: affected 1
//   23 relation-insert  oid 23, table "t", key [(1, [01])], values [(2, [0A])]
//                       ok: unit
//
//   err frame           get result Err(NotFound, "no such entry") with empty
//                       location and no source/backtrace; on the wire the
//                       UniError is [2, "no such entry", "\"None\"", "", []]
//                       (err_src is the host's JSON form of ErrorSource::None
//                       and err_details encodes as a MessagePack ARRAY, not
//                       bin — both quirks are pinned on purpose).
// ---------------------------------------------------------------------------

pub(crate) const SYSCALL_ALL_KIND_COUNT: usize = 23;
pub(crate) const SYSCALL_ALL_SEGMENT_COUNT: usize = 2 * SYSCALL_ALL_KIND_COUNT + 1;
pub(crate) const SYSCALL_ALL_ERR_RESULT: usize = SYSCALL_ALL_SEGMENT_COUNT - 1;

fn golden_all_oid(l: u64) -> UniOid {
    UniOid { h: 0, l }
}

fn golden_all_query_argv() -> UniQueryArgv {
    UniQueryArgv {
        oid: golden_all_oid(1),
        query: UniSqlStmt {
            sql_string: "select 1".to_string(),
        },
        param_list: UniSqlParam {
            params: Vec::new(),
            param_names: None,
        },
        // No param-desc in the golden corpus: the field is absent on the
        // wire, keeping every existing frame byte-identical.
        param_desc: None,
    }
}

fn golden_all_command_argv() -> UniCommandArgv {
    UniCommandArgv {
        oid: golden_all_oid(2),
        command: UniSqlStmt {
            sql_string: "update t set a = 1".to_string(),
        },
        param_list: UniSqlParam {
            params: Vec::new(),
            param_names: None,
        },
        param_desc: None,
    }
}

fn golden_all_batch_argv() -> UniCommandArgv {
    UniCommandArgv {
        oid: golden_all_oid(3),
        command: UniSqlStmt {
            sql_string: "insert into t values (1)".to_string(),
        },
        param_list: UniSqlParam {
            params: Vec::new(),
            param_names: None,
        },
        param_desc: None,
    }
}

pub(crate) fn golden_all_fs_open_argv() -> UniFsOpenArgv {
    UniFsOpenArgv {
        session: golden_all_oid(10),
        oid: golden_all_oid(11),
        path: "docs/a.txt".to_string(),
        flags: 2,
    }
}

fn golden_all_fs_stat() -> UniFsStat {
    UniFsStat {
        oid: golden_all_oid(5),
        generation: 1,
        entry: String::new(),
        length: 100,
        state: 1,
    }
}

fn golden_all_fs_dirents() -> Vec<UniFsDirent> {
    vec![
        UniFsDirent {
            name: "a.txt".to_string(),
            is_dir: false,
            length: 3,
        },
        UniFsDirent {
            name: "docs".to_string(),
            is_dir: true,
            length: 0,
        },
    ]
}

/// Key/value column lists for the relation requests: `[[attr, datum], ...]`.
pub(crate) fn golden_all_relation_get_key() -> Vec<(u64, Vec<u8>)> {
    vec![(1, vec![0x01]), (2, vec![0x02, 0x03])]
}

pub(crate) fn golden_all_relation_key() -> Vec<(u64, Vec<u8>)> {
    vec![(1, vec![0x01])]
}

pub(crate) fn golden_all_relation_values() -> Vec<(u64, Vec<u8>)> {
    vec![(2, vec![0x0A])]
}

/// Delta list for the `relation-update` request: `[[attr, op, datum], ...]`.
pub(crate) fn golden_all_relation_deltas() -> Vec<(u64, u8, Vec<u8>)> {
    vec![(3, 0, vec![0x05])]
}

/// Borrowed view of an owned column list for the relation encoders.
pub(crate) fn column_refs(columns: &[(u64, Vec<u8>)]) -> Vec<(u64, &[u8])> {
    columns
        .iter()
        .map(|(attr, datum)| (*attr, datum.as_slice()))
        .collect()
}

/// Borrowed view of an owned delta list for the relation-update encoder.
pub(crate) fn delta_refs(deltas: &[(u64, u8, Vec<u8>)]) -> Vec<(u64, u8, &[u8])> {
    deltas
        .iter()
        .map(|(attr, op, datum)| (*attr, *op, datum.as_slice()))
        .collect()
}

/// One frame of the all-kinds corpus: the canonical frame bytes plus the
/// semantic expectation the JSON sidecar shares with the non-Rust consumers
/// (C# / AssemblyScript). Both are derived from the same constructors below,
/// so the sidecar cannot drift from the bytes.
pub(crate) struct SyscallFrameCase {
    message_kind: MessageKind,
    /// `request`, `response` (ok), or `response_err` (UniError).
    direction: &'static str,
    pub(crate) frame: Vec<u8>,
    expect: serde_json::Value,
}

fn frame_case(
    message_kind: MessageKind,
    direction: &'static str,
    frame: Vec<u8>,
    expect: serde_json::Value,
) -> SyscallFrameCase {
    SyscallFrameCase {
        message_kind,
        direction,
        frame,
        expect,
    }
}

/// Lowercase hex rendering used for byte strings in the sidecars.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0F) as usize] as char);
    }
    out
}

/// Sidecar form of a `UniOid`: both halves as decimal strings (JSON numbers
/// cannot represent every u64 exactly).
fn oid_json(oid: &UniOid) -> serde_json::Value {
    json!({ "h": oid.h.to_string(), "l": oid.l.to_string() })
}

pub(crate) fn fs_open_argv_json(argv: &UniFsOpenArgv) -> serde_json::Value {
    json!({
        "session": oid_json(&argv.session),
        "oid": oid_json(&argv.oid),
        "path": argv.path,
        "flags": argv.flags,
    })
}

fn fs_stat_json(stat: &UniFsStat) -> serde_json::Value {
    json!({
        "oid": oid_json(&stat.oid),
        "generation": stat.generation.to_string(),
        "entry": stat.entry,
        "length": stat.length.to_string(),
        "state": stat.state,
    })
}

fn fs_dirents_json(entries: &[UniFsDirent]) -> serde_json::Value {
    entries
        .iter()
        .map(|entry| {
            json!({
                "name": entry.name,
                "is_dir": entry.is_dir,
                "length": entry.length.to_string(),
            })
        })
        .collect()
}

/// Sidecar form of a relation column list `[[attr, datum], ...]`.
fn columns_json(columns: &[(u64, Vec<u8>)]) -> serde_json::Value {
    columns
        .iter()
        .map(|(attr, datum)| json!({ "attr": attr.to_string(), "datum_hex": hex(datum) }))
        .collect()
}

/// Sidecar form of a relation delta list `[[attr, op, datum], ...]`.
fn deltas_json(deltas: &[(u64, u8, Vec<u8>)]) -> serde_json::Value {
    deltas
        .iter()
        .map(|(attr, op, datum)| {
            json!({ "attr": attr.to_string(), "op": op, "datum_hex": hex(datum) })
        })
        .collect()
}

/// Sidecar form of a relation row: an array of cells, each a hex string or
/// null; a null row means "no row found".
fn row_json(row: &Option<Vec<Option<Vec<u8>>>>) -> serde_json::Value {
    match row {
        None => serde_json::Value::Null,
        Some(cells) => cells
            .iter()
            .map(|cell| match cell {
                Some(bytes) => hex(bytes).into(),
                None => serde_json::Value::Null,
            })
            .collect(),
    }
}

/// The WIT name of a message kind, as used by guests and the sidecars.
pub(crate) fn message_kind_name(kind: MessageKind) -> &'static str {
    match kind {
        MessageKind::Query => "query",
        MessageKind::Command => "command",
        MessageKind::Batch => "batch",
        MessageKind::Open => "open-session",
        MessageKind::Close => "close-session",
        MessageKind::Get => "get",
        MessageKind::Put => "put",
        MessageKind::Delete => "delete",
        MessageKind::Range => "range",
        MessageKind::FsOpen => "fs-open",
        MessageKind::FsClose => "fs-close",
        MessageKind::FsRead => "fs-read",
        MessageKind::FsWrite => "fs-write",
        MessageKind::FsPread => "fs-pread",
        MessageKind::FsPwrite => "fs-pwrite",
        MessageKind::FsLseek => "fs-lseek",
        MessageKind::FsFstat => "fs-fstat",
        MessageKind::FsStat => "fs-stat",
        MessageKind::FsFsync => "fs-fsync",
        MessageKind::FsReaddir => "fs-readdir",
        MessageKind::RelationGet => "relation-get",
        MessageKind::RelationUpdate => "relation-update",
        MessageKind::RelationInsert => "relation-insert",
    }
}

/// Builds the 47 frames of the all-kinds corpus in segment order, each with
/// its semantic expectation (see the section comment above for the layout
/// and the sidecar conventions).
pub(crate) fn golden_syscall_all_cases() -> Vec<SyscallFrameCase> {
    let mut cases = Vec::new();
    cases.extend(sql_cases());
    cases.extend(session_cases());
    cases.extend(kv_cases());
    cases.extend(fs_open_close_cases());
    cases.extend(fs_rw_cases());
    cases.extend(fs_meta_cases());
    cases.extend(relation_cases());
    cases.push(err_result_case());
    assert_eq!(cases.len(), SYSCALL_ALL_SEGMENT_COUNT);
    cases
}

/// Kinds 1-3: query / command / batch.
fn sql_cases() -> Vec<SyscallFrameCase> {
    let mut cases = Vec::new();

    // 1 query: request [argv], ok result UniQueryResult default.
    let argv = golden_all_query_argv();
    cases.push(frame_case(
        MessageKind::Query,
        "request",
        encode_query_request(&argv),
        json!({
            "oid": oid_json(&argv.oid),
            "sql": argv.query.sql_string,
            "params": [],
        }),
    ));
    let result = UniQueryResult::default();
    cases.push(frame_case(
        MessageKind::Query,
        "response",
        encode_query_result(&Ok(result.clone())),
        json!({
            "record_name": result.tuple_desc.record_name,
            "fields": [],
            "eof": result.result_set.eof,
            "rows": [],
            "cursor_hex": hex(&result.result_set.cursor),
        }),
    ));

    // 2 command.
    let argv = golden_all_command_argv();
    cases.push(frame_case(
        MessageKind::Command,
        "request",
        encode_command_request(&argv),
        json!({
            "oid": oid_json(&argv.oid),
            "sql": argv.command.sql_string,
            "params": [],
        }),
    ));
    cases.push(frame_case(
        MessageKind::Command,
        "response",
        encode_command_result(&Ok(UniCommandResult { affected_rows: 3 })),
        json!({ "affected_rows": "3" }),
    ));

    // 3 batch.
    let argv = golden_all_batch_argv();
    cases.push(frame_case(
        MessageKind::Batch,
        "request",
        encode_batch_request(&argv),
        json!({
            "oid": oid_json(&argv.oid),
            "sql": argv.command.sql_string,
            "params": [],
        }),
    ));
    cases.push(frame_case(
        MessageKind::Batch,
        "response",
        encode_batch_result(&Ok(UniCommandResult { affected_rows: 2 })),
        json!({ "affected_rows": "2" }),
    ));

    cases
}

/// Kinds 4-5: open-session / close-session.
fn session_cases() -> Vec<SyscallFrameCase> {
    let mut cases = Vec::new();

    // 4 open-session.
    let worker = golden_all_oid(4);
    let expect = json!({ "worker_oid": oid_json(&worker) });
    cases.push(frame_case(
        MessageKind::Open,
        "request",
        encode_open_request(worker),
        expect,
    ));
    cases.push(frame_case(
        MessageKind::Open,
        "response",
        encode_open_result(&Ok(4)),
        json!({ "session": "4" }),
    ));

    // 5 close-session.
    let oid = golden_all_oid(5);
    let expect = json!({ "oid": oid_json(&oid) });
    cases.push(frame_case(
        MessageKind::Close,
        "request",
        encode_close_request(oid),
        expect,
    ));
    cases.push(frame_case(
        MessageKind::Close,
        "response",
        encode_close_result(&Ok(())),
        json!({ "unit": true }),
    ));

    cases
}

/// Kinds 6-9: get / put / delete / range.
fn kv_cases() -> Vec<SyscallFrameCase> {
    let mut cases = Vec::new();

    // 6 get.
    let oid = golden_all_oid(6);
    let expect = json!({ "oid": oid_json(&oid), "key_hex": hex(b"k1") });
    cases.push(frame_case(
        MessageKind::Get,
        "request",
        encode_get_request(oid, b"k1"),
        expect,
    ));
    let value = b"v1".to_vec();
    cases.push(frame_case(
        MessageKind::Get,
        "response",
        encode_get_result(&Ok(Some(value.clone()))),
        json!({ "value_hex": hex(&value) }),
    ));

    // 7 put.
    let oid = golden_all_oid(7);
    let expect = json!({ "oid": oid_json(&oid), "key_hex": hex(b"k1"), "value_hex": hex(b"v1") });
    cases.push(frame_case(
        MessageKind::Put,
        "request",
        encode_put_request(oid, b"k1", b"v1"),
        expect,
    ));
    cases.push(frame_case(
        MessageKind::Put,
        "response",
        encode_put_result(&Ok(())),
        json!({ "unit": true }),
    ));

    // 8 delete.
    let oid = golden_all_oid(8);
    let expect = json!({ "oid": oid_json(&oid), "key_hex": hex(b"k1") });
    cases.push(frame_case(
        MessageKind::Delete,
        "request",
        encode_delete_request(oid, b"k1"),
        expect,
    ));
    cases.push(frame_case(
        MessageKind::Delete,
        "response",
        encode_delete_result(&Ok(())),
        json!({ "unit": true }),
    ));

    // 9 range.
    let oid = golden_all_oid(9);
    let expect = json!({ "oid": oid_json(&oid), "start_hex": hex(b"a"), "end_hex": hex(b"z") });
    cases.push(frame_case(
        MessageKind::Range,
        "request",
        encode_range_request(oid, b"a", b"z"),
        expect,
    ));
    let items = vec![
        (b"a".to_vec(), b"1".to_vec()),
        (b"b".to_vec(), b"2".to_vec()),
    ];
    let items_json: serde_json::Value = items
        .iter()
        .map(|(key, value)| json!({ "key_hex": hex(key), "value_hex": hex(value) }))
        .collect();
    cases.push(frame_case(
        MessageKind::Range,
        "response",
        encode_range_result(&Ok(items)),
        json!({ "items": items_json }),
    ));

    cases
}

/// Kinds 10-11: fs-open / fs-close.
fn fs_open_close_cases() -> Vec<SyscallFrameCase> {
    let mut cases = Vec::new();

    // 10 fs-open.
    let argv = golden_all_fs_open_argv();
    cases.push(frame_case(
        MessageKind::FsOpen,
        "request",
        encode_fs_open_request(&argv),
        fs_open_argv_json(&argv),
    ));
    cases.push(frame_case(
        MessageKind::FsOpen,
        "response",
        encode_fs_open_result(&Ok(9)),
        json!({ "fd": 9 }),
    ));

    // 11 fs-close.
    cases.push(frame_case(
        MessageKind::FsClose,
        "request",
        encode_fs_close_request(3),
        json!({ "fd": 3 }),
    ));
    cases.push(frame_case(
        MessageKind::FsClose,
        "response",
        encode_fs_close_result(&Ok(())),
        json!({ "unit": true }),
    ));

    cases
}

/// Kinds 12-16: fs-read / fs-write / fs-pread / fs-pwrite / fs-lseek.
fn fs_rw_cases() -> Vec<SyscallFrameCase> {
    let mut cases = Vec::new();

    // 12 fs-read.
    cases.push(frame_case(
        MessageKind::FsRead,
        "request",
        encode_fs_read_request(3, 4),
        json!({ "fd": 3, "len": 4 }),
    ));
    cases.push(frame_case(
        MessageKind::FsRead,
        "response",
        encode_fs_read_result(&Ok(b"hi".to_vec())),
        json!({ "data_hex": hex(b"hi") }),
    ));

    // 13 fs-write.
    cases.push(frame_case(
        MessageKind::FsWrite,
        "request",
        encode_fs_write_request(3, b"hi"),
        json!({ "fd": 3, "data_hex": hex(b"hi") }),
    ));
    cases.push(frame_case(
        MessageKind::FsWrite,
        "response",
        encode_fs_write_result(&Ok(2)),
        json!({ "written": 2 }),
    ));

    // 14 fs-pread.
    cases.push(frame_case(
        MessageKind::FsPread,
        "request",
        encode_fs_pread_request(3, 8, 4),
        json!({ "fd": 3, "offset": "8", "len": 4 }),
    ));
    cases.push(frame_case(
        MessageKind::FsPread,
        "response",
        encode_fs_pread_result(&Ok(b"hi".to_vec())),
        json!({ "data_hex": hex(b"hi") }),
    ));

    // 15 fs-pwrite.
    cases.push(frame_case(
        MessageKind::FsPwrite,
        "request",
        encode_fs_pwrite_request(3, 8, b"hi"),
        json!({ "fd": 3, "offset": "8", "data_hex": hex(b"hi") }),
    ));
    cases.push(frame_case(
        MessageKind::FsPwrite,
        "response",
        encode_fs_pwrite_result(&Ok(())),
        json!({ "unit": true }),
    ));

    // 16 fs-lseek.
    cases.push(frame_case(
        MessageKind::FsLseek,
        "request",
        encode_fs_lseek_request(3, -2, 1),
        json!({ "fd": 3, "offset": "-2", "whence": 1 }),
    ));
    cases.push(frame_case(
        MessageKind::FsLseek,
        "response",
        encode_fs_lseek_result(&Ok(6)),
        json!({ "position": "6" }),
    ));

    cases
}

/// Kinds 17-20: fs-fstat / fs-stat / fs-fsync / fs-readdir.
fn fs_meta_cases() -> Vec<SyscallFrameCase> {
    let mut cases = Vec::new();

    // 17 fs-fstat.
    cases.push(frame_case(
        MessageKind::FsFstat,
        "request",
        encode_fs_fstat_request(3),
        json!({ "fd": 3 }),
    ));
    let stat = golden_all_fs_stat();
    cases.push(frame_case(
        MessageKind::FsFstat,
        "response",
        encode_fs_fstat_result(&Ok(stat.clone())),
        json!({ "stat": fs_stat_json(&stat) }),
    ));

    // 18 fs-stat.
    let oid = golden_all_oid(18);
    let expect = json!({ "oid": oid_json(&oid), "path": "a" });
    cases.push(frame_case(
        MessageKind::FsStat,
        "request",
        encode_fs_stat_request(oid, "a"),
        expect,
    ));
    let stat = golden_all_fs_stat();
    cases.push(frame_case(
        MessageKind::FsStat,
        "response",
        encode_fs_stat_result(&Ok(stat.clone())),
        json!({ "stat": fs_stat_json(&stat) }),
    ));

    // 19 fs-fsync.
    cases.push(frame_case(
        MessageKind::FsFsync,
        "request",
        encode_fs_fsync_request(3),
        json!({ "fd": 3 }),
    ));
    cases.push(frame_case(
        MessageKind::FsFsync,
        "response",
        encode_fs_fsync_result(&Ok(())),
        json!({ "unit": true }),
    ));

    // 20 fs-readdir.
    let oid = golden_all_oid(20);
    let expect = json!({ "oid": oid_json(&oid), "path": "d" });
    cases.push(frame_case(
        MessageKind::FsReaddir,
        "request",
        encode_fs_readdir_request(oid, "d"),
        expect,
    ));
    let entries = golden_all_fs_dirents();
    cases.push(frame_case(
        MessageKind::FsReaddir,
        "response",
        encode_fs_readdir_result(&Ok(entries.clone())),
        json!({ "entries": fs_dirents_json(&entries) }),
    ));

    cases
}

/// Kinds 21-23: relation-get / relation-update / relation-insert.
fn relation_cases() -> Vec<SyscallFrameCase> {
    let mut cases = Vec::new();

    // 21 relation-get.
    let oid = golden_all_oid(21);
    let key = golden_all_relation_get_key();
    let expect = json!({
        "oid": oid_json(&oid),
        "table": "t",
        "key": columns_json(&key),
        "select": ["3", "4"],
    });
    cases.push(frame_case(
        MessageKind::RelationGet,
        "request",
        encode_relation_get_request(oid, "t", &column_refs(&key), &[3, 4]),
        expect,
    ));
    let row = Some(vec![Some(vec![0x0A]), None]);
    cases.push(frame_case(
        MessageKind::RelationGet,
        "response",
        encode_relation_get_result(&Ok(row.clone())),
        json!({ "row": row_json(&row) }),
    ));

    // 22 relation-update.
    let oid = golden_all_oid(22);
    let key = golden_all_relation_key();
    let values = golden_all_relation_values();
    let deltas = golden_all_relation_deltas();
    let expect = json!({
        "oid": oid_json(&oid),
        "table": "t",
        "key": columns_json(&key),
        "values": columns_json(&values),
        "deltas": deltas_json(&deltas),
    });
    cases.push(frame_case(
        MessageKind::RelationUpdate,
        "request",
        encode_relation_update_request(
            oid,
            "t",
            &column_refs(&key),
            &column_refs(&values),
            &delta_refs(&deltas),
        ),
        expect,
    ));
    cases.push(frame_case(
        MessageKind::RelationUpdate,
        "response",
        encode_relation_update_result(&Ok(1)),
        json!({ "affected": "1" }),
    ));

    // 23 relation-insert.
    let oid = golden_all_oid(23);
    let key = golden_all_relation_key();
    let values = golden_all_relation_values();
    let expect = json!({
        "oid": oid_json(&oid),
        "table": "t",
        "key": columns_json(&key),
        "values": columns_json(&values),
    });
    cases.push(frame_case(
        MessageKind::RelationInsert,
        "request",
        encode_relation_insert_request(oid, "t", &column_refs(&key), &column_refs(&values)),
        expect,
    ));
    cases.push(frame_case(
        MessageKind::RelationInsert,
        "response",
        encode_relation_insert_result(&Ok(())),
        json!({ "unit": true }),
    ));

    cases
}

/// Trailing UniError frame: get result with a NotFound error.
fn err_result_case() -> SyscallFrameCase {
    let error = golden_syscall_error();
    let expect = json!({
        "err_code": error.ec().to_u32(),
        "err_msg": error.message(),
        // The host JSON form of ErrorSource::None, pinned by the corpus.
        "err_src": "\"None\"",
        "err_loc": error.loc(),
        "err_details_hex": "",
    });
    frame_case(
        MessageKind::Get,
        "response_err",
        encode_get_result(&Err(error)),
        expect,
    )
}

/// JSON sidecar schema of the all-kinds corpus: one entry per segment.
#[derive(Serialize)]
struct SyscallCorpusEntry {
    index: usize,
    message_kind: u32,
    message_kind_name: &'static str,
    direction: &'static str,
    expect: serde_json::Value,
}

/// Value conventions used by the `expect` objects, mirrored into the sidecar
/// so non-Rust consumers can rebuild every expected value.
#[derive(Serialize)]
pub(crate) struct SyscallCorpusConventions {
    pub(crate) integers: &'static str,
    pub(crate) byte_strings: &'static str,
    pub(crate) oids: &'static str,
    pub(crate) unit: &'static str,
    pub(crate) relation_cells: &'static str,
    pub(crate) direction: &'static str,
}

#[derive(Serialize)]
struct SyscallCorpusSidecar {
    format: &'static str,
    reference: &'static str,
    container: &'static str,
    regenerate: &'static str,
    conventions: SyscallCorpusConventions,
    frames: Vec<SyscallCorpusEntry>,
}

pub(crate) fn syscall_all_sidecar_json(cases: &[SyscallFrameCase]) -> String {
    let sidecar = SyscallCorpusSidecar {
        format: "syscall-payload-v1-all",
        reference: "mudu_binding::codec::syscall_payload (the host MSSP v1 codec is authoritative)",
        container: "syscall_payload_v1_all.bin: one segment per frame, each a 4-byte big-endian \
                    u32 length followed by the canonical MSSP frame (16-byte header + MessagePack \
                    body); segment order matches the frames array below",
        regenerate: "cargo test -p testing --test compat_golden -- generate_golden_v1_fixtures --ignored",
        conventions: SyscallCorpusConventions {
            integers: "u64/i64/u128 values are decimal strings; u8/u32 values are plain JSON numbers",
            byte_strings: "byte strings are lowercase hex in *_hex fields (empty string = empty bytes)",
            oids: "a UniOid is {\"h\": ..., \"l\": ...} with both u64 halves as decimal strings",
            unit: "a unit (ok, no payload) result is {\"unit\": true}",
            relation_cells: "a relation row is an array of cells; each cell is a hex string or \
                             null; a null row means no row found",
            direction: "request = guest→host call, response = ok result, response_err = UniError result",
        },
        frames: cases
            .iter()
            .enumerate()
            .map(|(index, case)| SyscallCorpusEntry {
                index,
                message_kind: u32::from(case.message_kind),
                message_kind_name: message_kind_name(case.message_kind),
                direction: case.direction,
                expect: case.expect.clone(),
            })
            .collect(),
    };
    let mut json = serde_json::to_string_pretty(&sidecar).unwrap();
    json.push('\n');
    json
}

/// Asserts the ok-result roundtrip of one canonical response frame:
/// decode → re-encode → decode must reach a stable canonical form (the second
/// decode re-encodes to the same bytes as the first). Unlike the request
/// frames, response frames are pinned semantically, not byte-exactly.
pub(crate) fn assert_ok_result_roundtrip<T>(
    segment: &[u8],
    decode: impl Fn(&[u8]) -> RS<T>,
    encode: impl Fn(&RS<T>) -> Vec<u8>,
) {
    let value = decode(segment).expect("decode ok result");
    let reencoded = encode(&Ok(value));
    let redecoded = decode(&reencoded).expect("re-decode re-encoded result");
    assert_eq!(
        encode(&Ok(redecoded)),
        reencoded,
        "response decode → re-encode → decode must be a stable fixpoint"
    );
}
