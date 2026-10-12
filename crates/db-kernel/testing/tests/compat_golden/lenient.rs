use super::syscall_all_cases::{
    SyscallCorpusConventions, fs_open_argv_json, golden_all_fs_open_argv, message_kind_name,
};
use super::*;

// ---------------------------------------------------------------------------
// Lenient decode vectors (lenient_decode_v1)
//
// `lenient_decode_v1.bin` pins the LENIENT side of the MSSP v1 decoder
// (`mudu_binding::universal::mp_wire` + the generated per-function codecs):
// each segment is a complete MSSP frame whose body is legal but NOT
// canonical — byte shapes the canonical encoder can never emit, which the
// decoder must still accept. The container is the same big-endian-u32
// length-prefixed segment layout as the syscall corpus; segment order
// matches the `vectors` array of the JSON sidecar `lenient_decode_v1.json`,
// which lists (index, kind, message_kind, message_kind_name, direction,
// note, expect) per vector so non-Rust consumers can replay the leniency
// rules without a Rust toolchain.
//
// The frames are hand-built below (the canonical encoder cannot produce
// them). Every byte literal is annotated; the pinned rules are:
//
//   - integers of any MessagePack width are accepted (non-minimal encodings
//     decode to their value, signed/unsigned marker forms are
//     interchangeable within range);
//   - record and request maps tolerate out-of-order keys, unknown field
//     numbers, non-integer keys (skipped), and missing fields/parameters
//     (proto3-style defaults, including the default case of a variant
//     field).
//
// Regeneration (writes both fixture files; run once and commit):
//
//   cargo test -p testing --test compat_golden -- generate_golden_v1_fixtures --ignored
// ---------------------------------------------------------------------------

/// One lenient decode vector: a hand-built non-canonical frame plus the
/// semantics it must decode to.
pub(crate) struct LenientVector {
    /// Leniency category exercised by the vector.
    kind: &'static str,
    message_kind: MessageKind,
    /// `request` or `response` (which codec pair consumes the frame).
    direction: &'static str,
    /// Human-readable note on the non-canonical aspect.
    note: &'static str,
    /// Hand-built non-canonical MSSP frame (header + body).
    pub(crate) frame: Vec<u8>,
    /// Expected decoded semantics (mirrored into the JSON sidecar).
    expect: serde_json::Value,
}

/// Wraps a hand-built MessagePack body in a valid MSSP header.
fn lenient_frame(kind: MessageKind, body: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(HEADER_LEN + body.len());
    frame.extend_from_slice(&encode_header(kind));
    frame.extend_from_slice(body);
    frame
}

/// Hand-built UniOid record map `{1: h, 2: l}` with fixint values; only
/// valid for the small corpus oids (both halves ≤ 0x7F).
fn oid_map_bytes(oid: &UniOid) -> Vec<u8> {
    assert!(oid.h <= 0x7F && oid.l <= 0x7F, "fixint oids only");
    vec![0x82, 0x01, oid.h as u8, 0x02, oid.l as u8]
}

/// Hand-built fixstr bytes; only valid for strings shorter than 32 bytes.
fn fixstr_bytes(s: &str) -> Vec<u8> {
    assert!(s.len() < 32, "fixstr only");
    let mut out = vec![0xA0 | s.len() as u8];
    out.extend_from_slice(s.as_bytes());
    out
}

/// The 8 lenient decode vectors in segment order.
pub(crate) fn lenient_vectors() -> Vec<LenientVector> {
    let mut vectors = Vec::new();
    vectors.extend(int_width_vectors());
    vectors.extend(fs_open_record_vectors());
    vectors.extend(missing_field_vectors());
    vectors
}

/// Vectors 0-2: non-minimal / cross-sign MessagePack integer markers.
fn int_width_vectors() -> Vec<LenientVector> {
    let mut vectors = Vec::new();

    // 0. fs-write ok result whose `written = 2` uses the non-minimal u32
    //    marker 0xCE (canonical: fixint 0x02).
    let mut body = vec![0x92, 0x00, 0xCE];
    body.extend_from_slice(&2u32.to_be_bytes());
    vectors.push(LenientVector {
        kind: "int-width-widening",
        message_kind: MessageKind::FsWrite,
        direction: "response",
        note: "u64 value 2 encoded with the u32 marker 0xCE instead of fixint 0x02",
        frame: lenient_frame(MessageKind::FsWrite, &body),
        expect: json!({ "written": 2 }),
    });

    // 1. fs-lseek request whose non-negative i64 offset 8 uses the unsigned
    //    u8 marker 0xCC (canonical: fixint 0x08).
    let body = vec![0x83, 0x01, 0x03, 0x02, 0xCC, 0x08, 0x03, 0x01];
    vectors.push(LenientVector {
        kind: "int-unsigned-marker",
        message_kind: MessageKind::FsLseek,
        direction: "request",
        note: "i64 offset 8 encoded with the unsigned u8 marker 0xCC instead of fixint 0x08",
        frame: lenient_frame(MessageKind::FsLseek, &body),
        expect: json!({ "fd": 3, "offset": "8", "whence": 1 }),
    });

    // 2. fs-lseek request whose small negative i64 offset -2 uses the wide
    //    i64 marker 0xD3 (canonical: negfixint 0xFE).
    let mut body = vec![0x83, 0x01, 0x03, 0x02, 0xD3];
    body.extend_from_slice(&(-2i64).to_be_bytes());
    body.extend_from_slice(&[0x03, 0x01]);
    vectors.push(LenientVector {
        kind: "int-wide-negative",
        message_kind: MessageKind::FsLseek,
        direction: "request",
        note: "i64 offset -2 encoded with the i64 marker 0xD3 instead of negfixint 0xFE",
        frame: lenient_frame(MessageKind::FsLseek, &body),
        expect: json!({ "fd": 3, "offset": "-2", "whence": 1 }),
    });

    vectors
}

/// Vectors 3-5: fs-open argv record maps with out-of-order, unknown, or
/// non-integer keys.
fn fs_open_record_vectors() -> Vec<LenientVector> {
    let mut vectors = Vec::new();

    // 3. fs-open request whose argv record map lists the keys out of order
    //    (4, 3, 2, 1 instead of 1, 2, 3, 4). The request body itself is the
    //    single-parameter map {1: argv}.
    let argv = golden_all_fs_open_argv();
    let mut body = vec![0x81, 0x01, 0x84];
    body.push(0x04);
    body.push(argv.flags as u8);
    body.push(0x03);
    body.extend_from_slice(&fixstr_bytes(&argv.path));
    body.push(0x02);
    body.extend_from_slice(&oid_map_bytes(&argv.oid));
    body.push(0x01);
    body.extend_from_slice(&oid_map_bytes(&argv.session));
    vectors.push(LenientVector {
        kind: "record-key-order",
        message_kind: MessageKind::FsOpen,
        direction: "request",
        note: "fs-open argv record map keys in 4,3,2,1 order instead of 1,2,3,4",
        frame: lenient_frame(MessageKind::FsOpen, &body),
        expect: fs_open_argv_json(&argv),
    });

    // 4. fs-open request whose argv record map carries an extra unknown
    //    field number 99 (value [1, 2]); the decoder must skip it.
    let argv = golden_all_fs_open_argv();
    let mut body = vec![0x81, 0x01, 0x85];
    body.push(0x01);
    body.extend_from_slice(&oid_map_bytes(&argv.session));
    body.push(0x02);
    body.extend_from_slice(&oid_map_bytes(&argv.oid));
    body.push(0x03);
    body.extend_from_slice(&fixstr_bytes(&argv.path));
    body.push(0x04);
    body.push(argv.flags as u8);
    body.push(0x63); // key 99
    body.extend_from_slice(&[0x92, 0x01, 0x02]); // skipped value [1, 2]
    vectors.push(LenientVector {
        kind: "record-unknown-field",
        message_kind: MessageKind::FsOpen,
        direction: "request",
        note: "fs-open argv record map with an unknown field number 99 (skipped)",
        frame: lenient_frame(MessageKind::FsOpen, &body),
        expect: fs_open_argv_json(&argv),
    });

    // 5. fs-open request whose argv record map carries a non-integer key
    //    ("note" → "x"); the decoder must skip the pair.
    let argv = golden_all_fs_open_argv();
    let mut body = vec![0x81, 0x01, 0x85];
    body.push(0x01);
    body.extend_from_slice(&oid_map_bytes(&argv.session));
    body.push(0x02);
    body.extend_from_slice(&oid_map_bytes(&argv.oid));
    body.push(0x03);
    body.extend_from_slice(&fixstr_bytes(&argv.path));
    body.push(0x04);
    body.push(argv.flags as u8);
    body.extend_from_slice(&fixstr_bytes("note")); // non-integer key
    body.extend_from_slice(&fixstr_bytes("x"));
    vectors.push(LenientVector {
        kind: "map-noninteger-key",
        message_kind: MessageKind::FsOpen,
        direction: "request",
        note: "fs-open argv map with a string key \"note\" (skipped)",
        frame: lenient_frame(MessageKind::FsOpen, &body),
        expect: fs_open_argv_json(&argv),
    });

    vectors
}

/// Vectors 6-7: missing parameters / fields decode to proto3-style defaults.
fn missing_field_vectors() -> Vec<LenientVector> {
    let mut vectors = Vec::new();

    // 6. fs-read request missing parameter 2 (len); the decoder must use the
    //    proto3-style default 0.
    let body = vec![0x81, 0x01, 0x03];
    vectors.push(LenientVector {
        kind: "request-missing-param",
        message_kind: MessageKind::FsRead,
        direction: "request",
        note: "fs-read request without parameter 2 (len defaults to 0)",
        frame: lenient_frame(MessageKind::FsRead, &body),
        expect: json!({ "fd": 3, "len": 0 }),
    });

    // 7. query ok result [0, {1: {1: "t", 2: [{1: "a"}]}}]: the record field
    //    omits its variant `field_type` (default case scalar(bool)) and
    //    `field_attrs`, and the whole `result_set` field is absent (all
    //    proto3-style defaults).
    let mut body = vec![0x92, 0x00, 0x81, 0x01, 0x82];
    body.push(0x01);
    body.extend_from_slice(&fixstr_bytes("t"));
    body.push(0x02);
    body.extend_from_slice(&[0x91, 0x81, 0x01]);
    body.extend_from_slice(&fixstr_bytes("a"));
    vectors.push(LenientVector {
        kind: "record-missing-fields",
        message_kind: MessageKind::Query,
        direction: "response",
        note: "query result whose record field omits the variant field_type (default case \
               scalar(bool)) and field_attrs, and whose result_set is absent",
        frame: lenient_frame(MessageKind::Query, &body),
        expect: json!({
            "record_name": "t",
            "fields": [
                { "field_name": "a", "field_type": "scalar(bool)", "field_attrs": [] }
            ],
            "eof": false,
            "rows": [],
            "cursor_hex": "",
        }),
    });

    vectors
}

/// JSON sidecar schema of the lenient decode corpus: one entry per segment.
#[derive(Serialize)]
struct LenientCorpusEntry {
    index: usize,
    kind: &'static str,
    message_kind: u32,
    message_kind_name: &'static str,
    direction: &'static str,
    note: &'static str,
    expect: serde_json::Value,
}

#[derive(Serialize)]
struct LenientCorpusSidecar {
    format: &'static str,
    reference: &'static str,
    container: &'static str,
    regenerate: &'static str,
    conventions: SyscallCorpusConventions,
    vectors: Vec<LenientCorpusEntry>,
}

pub(crate) fn lenient_sidecar_json(vectors: &[LenientVector]) -> String {
    let sidecar = LenientCorpusSidecar {
        format: "lenient-decode-v1",
        reference: "mudu_binding::universal::mp_wire lenient decode rules (proto3-flavored)",
        container: "lenient_decode_v1.bin: one segment per vector, each a 4-byte big-endian u32 \
                    length followed by a complete hand-built NON-canonical MSSP frame (16-byte \
                    header + MessagePack body); segment order matches the vectors array below",
        regenerate: "cargo test -p testing --test compat_golden -- generate_golden_v1_fixtures --ignored",
        conventions: SyscallCorpusConventions {
            integers: "u64/i64/u128 values are decimal strings; u8/u32 values are plain JSON numbers",
            byte_strings: "byte strings are lowercase hex in *_hex fields (empty string = empty bytes)",
            oids: "a UniOid is {\"h\": ..., \"l\": ...} with both u64 halves as decimal strings",
            unit: "a unit (ok, no payload) result is {\"unit\": true}",
            relation_cells: "a relation row is an array of cells; each cell is a hex string or \
                             null; a null row means no row found",
            direction: "request = decode with the request codec, response = decode with the result codec",
        },
        vectors: vectors
            .iter()
            .enumerate()
            .map(|(index, vector)| LenientCorpusEntry {
                index,
                kind: vector.kind,
                message_kind: u32::from(vector.message_kind),
                message_kind_name: message_kind_name(vector.message_kind),
                direction: vector.direction,
                note: vector.note,
                expect: vector.expect.clone(),
            })
            .collect(),
    };
    let mut json = serde_json::to_string_pretty(&sidecar).unwrap();
    json.push('\n');
    json
}

/// Asserts the fs-open lenient vectors decode to the same argv as the
/// canonical corpus constructor.
fn assert_lenient_fs_open(segment: &[u8]) {
    let decoded = decode_fs_open_request(segment).expect("decode lenient fs-open request");
    let expected = golden_all_fs_open_argv();
    assert_eq!(
        (decoded.session.h, decoded.session.l),
        (expected.session.h, expected.session.l)
    );
    assert_eq!(
        (decoded.oid.h, decoded.oid.l),
        (expected.oid.h, expected.oid.l)
    );
    assert_eq!(decoded.path, expected.path);
    assert_eq!(decoded.flags, expected.flags);
}

/// Verifies the lenient decode corpus: every hand-built non-canonical frame
/// must decode to its documented semantics, and the committed sidecar must
/// be reproducible from the same vector table.
#[test]
fn lenient_decode_v1_vectors() {
    let bin = mudu_sys::fs::sync::sync_read_all(fixture_path("lenient_decode_v1.bin"))
        .expect("missing lenient_decode_v1.bin; run generate_golden_v1_fixtures");
    let json_bytes = mudu_sys::fs::sync::sync_read_all(fixture_path("lenient_decode_v1.json"))
        .expect("missing lenient_decode_v1.json; run generate_golden_v1_fixtures");
    let vectors = lenient_vectors();
    assert_eq!(
        json_bytes,
        lenient_sidecar_json(&vectors).into_bytes(),
        "lenient sidecar drift; rerun generate_golden_v1_fixtures"
    );

    let segments = unpack_syscall_segments(&bin);
    assert_eq!(segments.len(), vectors.len());
    for (vector, segment) in vectors.iter().zip(segments.iter()) {
        assert_eq!(
            vector.frame.as_slice(),
            *segment,
            "lenient vector bytes drift; rerun generate_golden_v1_fixtures"
        );
        assert_eq!(decode_header(segment).unwrap(), vector.message_kind);
        match vector.kind {
            "int-width-widening" => {
                assert_eq!(decode_fs_write_result(segment).unwrap(), 2)
            }
            "int-unsigned-marker" => {
                assert_eq!(decode_fs_lseek_request(segment).unwrap(), (3, 8, 1))
            }
            "int-wide-negative" => {
                assert_eq!(decode_fs_lseek_request(segment).unwrap(), (3, -2, 1))
            }
            "record-key-order" | "record-unknown-field" | "map-noninteger-key" => {
                assert_lenient_fs_open(segment)
            }
            "request-missing-param" => {
                assert_eq!(decode_fs_read_request(segment).unwrap(), (3, 0))
            }
            "record-missing-fields" => {
                let result = decode_query_result(segment).expect("decode lenient query result");
                assert_eq!(result.tuple_desc.record_name, "t");
                assert_eq!(result.tuple_desc.record_fields.len(), 1);
                let field = &result.tuple_desc.record_fields[0];
                assert_eq!(field.field_name, "a");
                // The missing variant field decodes to its default case.
                assert!(matches!(
                    field.field_type,
                    UniDataType::Scalar(UniScalar::Bool)
                ));
                assert!(field.field_attrs.is_empty());
                assert!(!result.result_set.eof);
                assert!(result.result_set.row_set.is_empty());
                assert!(result.result_set.cursor.is_empty());
            }
            other => panic!("unknown lenient vector kind {other}"),
        }
    }
}
