use super::*;

// ---------------------------------------------------------------------------
// SyscallPayload v1 (MSSP) fixture
//
// `syscall_payload_v1.bin` packs five canonical frames as consecutive
// segments, each prefixed with its big-endian u32 byte length:
//
//   0. `get` request           [oid, key]
//   1. `get` ok result         [0, value]
//   2. `get` error result      [1, UniError]
//   3. `fs-open` request       [argv]
//   4. `fs-readdir` ok result  [0, [UniFsDirent, ...]]
//
// The same constructor functions feed the one-shot generator and the
// verification test, so the committed bytes always mirror the current
// encoder.  The error is built with an explicit empty location and no
// source/backtrace so the encoded bytes do not depend on the call site.
// ---------------------------------------------------------------------------

pub(crate) const GET_REQUEST: usize = 0;
const GET_OK_RESULT: usize = 1;
const GET_ERR_RESULT: usize = 2;
const FS_OPEN_REQUEST: usize = 3;
const FS_READDIR_RESULT: usize = 4;
const SYSCALL_SEGMENT_COUNT: usize = 5;

fn golden_syscall_oid() -> UniOid {
    UniOid {
        h: 0x0102_0304_0506_0708,
        l: 0x1112_1314_1516_1718,
    }
}

pub(crate) fn golden_syscall_error() -> MuduError {
    MuduError::new(
        ErrorCode::NotFound,
        "no such entry",
        None,
        String::new(),
        None,
    )
}

/// Builds the five canonical syscall frames in fixture segment order.
pub(crate) fn golden_syscall_frames() -> [Vec<u8>; SYSCALL_SEGMENT_COUNT] {
    [
        encode_get_request(golden_syscall_oid(), b"golden-key"),
        encode_get_result(&Ok(Some(b"golden-value".to_vec()))),
        encode_get_result(&Err(golden_syscall_error())),
        encode_fs_open_request(&UniFsOpenArgv {
            session: golden_syscall_oid(),
            oid: golden_syscall_oid(),
            path: "/golden/data.bin".to_string(),
            flags: 3,
        }),
        encode_fs_readdir_result(&Ok(vec![
            UniFsDirent {
                name: "dir".to_string(),
                is_dir: true,
                length: 0,
            },
            UniFsDirent {
                name: "file.txt".to_string(),
                is_dir: false,
                length: 12,
            },
        ])),
    ]
}

/// Verifies that the committed v1 golden fixtures decode with the current code.
#[test]
fn golden_v1_roundtrips() {
    let page_bytes = mudu_sys::fs::sync::sync_read_all(fixture_path("page_header_v1.bin"))
        .expect("missing page_header_v1.bin; run generate_golden_v1_fixtures");
    let header = PageHeader::decode(&page_bytes).expect("decode page header");
    assert_eq!(header.page_id(), 42);
    assert_eq!(header.version(), 1);

    let log_bytes = mudu_sys::fs::sync::sync_read_all(fixture_path("log_frame_v1.bin"))
        .expect("missing log_frame_v1.bin; run generate_golden_v1_fixtures");
    let payload: LogPayload = deserialize_entry(&[log_bytes]).expect("decode log frame payload");
    assert_eq!(payload.value, 12345);
    assert_eq!(payload.text, "golden");

    let proto_bytes = mudu_sys::fs::sync::sync_read_all(fixture_path("protocol_frame_v1.bin"))
        .expect("missing protocol_frame_v1.bin; run generate_golden_v1_fixtures");
    let frame = Frame::decode(&proto_bytes).expect("decode protocol frame");
    assert_eq!(frame.header().message_type(), MessageType::Query);
    assert_eq!(frame.header().request_id(), 42);
    assert_eq!(frame.payload(), b"select 1");

    let syscall_bytes = mudu_sys::fs::sync::sync_read_all(fixture_path("syscall_payload_v1.bin"))
        .expect("missing syscall_payload_v1.bin; run generate_golden_v1_fixtures");
    // The committed bytes must match the current encoder byte for byte.
    let frames = golden_syscall_frames();
    assert_eq!(syscall_bytes, pack_syscall_segments(&frames));
    let segments = unpack_syscall_segments(&syscall_bytes);
    assert_eq!(segments.len(), SYSCALL_SEGMENT_COUNT);

    // Segment 0: get request [oid, key].
    let (oid, key) = decode_get_request(segments[GET_REQUEST]).expect("decode get request");
    assert_eq!(oid.h, golden_syscall_oid().h);
    assert_eq!(oid.l, golden_syscall_oid().l);
    assert_eq!(key, b"golden-key");
    assert_eq!(
        encode_get_request(oid, &key).as_slice(),
        segments[GET_REQUEST]
    );

    // Segment 1: get ok result [0, value].
    let value = decode_get_result(segments[GET_OK_RESULT]).expect("decode get ok result");
    assert_eq!(value, Some(b"golden-value".to_vec()));
    assert_eq!(
        encode_get_result(&Ok(value)).as_slice(),
        segments[GET_OK_RESULT]
    );

    // Segment 2: get error result [1, UniError].  The whole-file byte
    // equality above already pins this frame against the current encoder;
    // here the decode side must carry the error code and message.  (A
    // decoded error cannot re-encode byte-exactly because decoding assigns
    // it a fresh caller location.)
    let err = decode_get_result(segments[GET_ERR_RESULT]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotFound);
    assert_eq!(err.message(), "no such entry");

    // Segment 3: fs-open request [argv].
    let argv = decode_fs_open_request(segments[FS_OPEN_REQUEST]).expect("decode fs-open request");
    assert_eq!(argv.session.h, golden_syscall_oid().h);
    assert_eq!(argv.path, "/golden/data.bin");
    assert_eq!(argv.flags, 3);
    assert_eq!(
        encode_fs_open_request(&argv).as_slice(),
        segments[FS_OPEN_REQUEST]
    );

    // Segment 4: fs-readdir ok result [0, [UniFsDirent, ...]].
    let entries =
        decode_fs_readdir_result(segments[FS_READDIR_RESULT]).expect("decode fs-readdir result");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "dir");
    assert!(entries[0].is_dir);
    assert_eq!(entries[1].name, "file.txt");
    assert_eq!(entries[1].length, 12);
    assert_eq!(
        encode_fs_readdir_result(&Ok(entries)).as_slice(),
        segments[FS_READDIR_RESULT]
    );
}
