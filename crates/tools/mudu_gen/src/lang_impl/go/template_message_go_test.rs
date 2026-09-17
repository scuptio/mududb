//! Smoke tests for the Go message back-end: record/variant/enum rendering
//! plus the MSSP func codecs, driven through the public [`CodeGen`] API
//! (the same path `mgen message -l go --with-func-codec` takes).

use crate::src_gen::code_gen::CodeGen;
use crate::src_gen::codegen_cfg::CodegenCfg;
use mudu::common::result::RS;

const TYPES_WIT: &str = r#"
interface universal {
    // object id
    record uni-oid {
        // higher 64 bits
        h: u64,
        // lower 64 bits
        l: u64,
    }

    enum uni-scalar {
        bool,
        %u8,
        %string,
    }

    variant uni-data-value {
        scalar(string),
        binary(list<u8>),
        null,
    }

    record uni-message {
        message-id: u32,
        source-oid: uni-oid,
        payload: list<u8>,
        kind: option<uni-scalar>,
        tags: list<string>,
    }
}
"#;

const SYSCALL_WIT: &str = r#"
interface universal {
    // Shared error record; note that the tree-sitter-wit grammar accepts
    // comments before records but not directly before `func` items.
    record uni-error {
        err-code: u32,
        err-msg: string,
    }

    get: func(oid: u64, key: list<u8>) -> result<option<list<u8>>, uni-error>;
    put: func(oid: u64, key: list<u8>, value: list<u8>) -> result<_, uni-error>;
}
"#;

fn generate(wit: &str, with_func_codec: bool) -> RS<String> {
    let mut cfg = CodegenCfg::new();
    cfg.with_func_codec = with_func_codec;
    CodeGen::generate_message_code_from_wit_with_cfg(wit, "go", None, cfg)
}

// Miri cannot execute FFI calls into the tree-sitter C parser, so skip these
// tests under Miri. Code generation is still exercised by normal cargo test.
#[test]
#[cfg_attr(miri, ignore)]
fn go_message_renders_record_variant_enum() -> RS<()> {
    let src = generate(TYPES_WIT, false)?;
    assert!(src.starts_with("package types"), "{src}");
    // record: exported struct, integer-keyed map codec
    assert!(src.contains("type UniOid struct {"), "{src}");
    assert!(src.contains("H uint64"), "{src}");
    assert!(
        src.contains("func UniOidToValue(x UniOid) (any, error) {"),
        "{src}"
    );
    assert!(
        src.contains("func UniOidFromValue(v any) (UniOid, error) {"),
        "{src}"
    );
    // record-context blob encodes as an array, not a bin
    assert!(src.contains("_d[3] = blobToArrayValue(x.Payload)"), "{src}");
    // option field is omitted when absent
    assert!(src.contains("if x.Kind != nil {"), "{src}");
    // enum: uint32 const block plus discriminant codec (const members are
    // gofmt-aligned, hence the padded member name)
    assert!(src.contains("type UniScalar uint32"), "{src}");
    assert!(src.contains("UniScalarU8     UniScalar = 1"), "{src}");
    assert!(
        src.contains("func UniScalarFromValue(v any) (UniScalar, error) {"),
        "{src}"
    );
    // variant: sealed interface, one struct per case, [tag, payload] codec
    assert!(src.contains("type UniDataValue interface {"), "{src}");
    assert!(src.contains("type UniDataValueBinary struct {"), "{src}");
    assert!(src.contains("type UniDataValueNull struct{}"), "{src}");
    assert!(
        src.contains("func defaultUniDataValue() UniDataValue {"),
        "{src}"
    );
    assert!(
        src.contains("return []any{uint64(2), uint8(0)}, nil"),
        "{src}"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore)]
fn go_message_renders_func_codecs() -> RS<()> {
    let src = generate(SYSCALL_WIT, true)?;
    // the shared preamble: pinned message kinds, WireResult, header decode
    assert!(src.contains("type MessageKind uint32"), "{src}");
    assert!(src.contains("MessageKindGet MessageKind = 1"), "{src}");
    assert!(src.contains("MessageKindPut MessageKind = 2"), "{src}");
    assert!(src.contains("type WireResult struct {"), "{src}");
    assert!(src.contains("Error *UniError"), "{src}");
    assert!(
        src.contains("func DecodeHeader(frame []byte) (MessageKind, error) {"),
        "{src}"
    );
    // request codecs: func-level list<u8> travels as a bin (the []byte
    // itself), NOT as an array
    assert!(
        src.contains("func EncodeGetRequest(oid uint64, key []byte) ([]byte, error) {"),
        "{src}"
    );
    assert!(src.contains("_body[2] = key"), "{src}");
    assert!(
        src.contains("func DecodeGetRequest(frame []byte) (uint64, []byte, error) {"),
        "{src}"
    );
    // result codecs: option ok payload unwraps nil; unit ok is [0, 0]
    assert!(
        src.contains("func EncodeGetResult(result WireResult) ([]byte, error) {"),
        "{src}"
    );
    assert!(src.contains("if result.Value == nil {"), "{src}");
    assert!(
        src.contains("codec.EncodeUnitResultOkFrame(uint32(MessageKindPut))"),
        "{src}"
    );
    assert!(
        src.contains("func DecodePutResult(frame []byte) (WireResult, error) {"),
        "{src}"
    );
    // the runtime import is pulled in when func codecs reference it
    assert!(
        src.contains("\"github.com/ybbh/mududb_p/bindings/go/codec\""),
        "{src}"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore)]
fn go_message_output_is_byte_stable() -> RS<()> {
    let a = generate(TYPES_WIT, true)?;
    let b = generate(TYPES_WIT, true)?;
    assert_eq!(a, b);
    let a = generate(SYSCALL_WIT, true)?;
    let b = generate(SYSCALL_WIT, true)?;
    assert_eq!(a, b);
    Ok(())
}
