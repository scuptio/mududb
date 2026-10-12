use super::*;

// ---------------------------------------------------------------------------
// MessagePack primitive alignment corpus (mp_primitives_v1)
//
// `mp_primitives_v1.bin` pins the rmp_serde 1.3.1 primitive encoding rules as
// a cross-language golden file, complementing the syscall frame corpus above
// (which pins whole frames).  Each test vector is one segment in the same
// big-endian-u32 length-prefixed container as the syscall corpus; segment
// order matches the `vectors` array of the JSON sidecar
// `mp_primitives_v1.json`, which lists `(index, kind, value|len)` per
// segment so the C# and AssemblyScript consumers can rebuild each expected
// value without a Rust toolchain.
//
// Pinned rules (verified against rmp_serde 1.3.1, the repo's locked version):
//
//   - Integers use minimal-width encoding BY VALUE, independent of the Rust
//     source type.  u64: fixint / 0xCC / 0xCD / 0xCE / 0xCF at the
//     127 / 255 / 65535 / 2^32-1 boundaries.  i64 non-negative values use the
//     UNSIGNED marker chain (128i64 encodes as 0xCC 0x80); negative values
//     use negfixint (-1..-32) / 0xD0 / 0xD1 / 0xD2 / 0xD3.
//   - Strings: fixstr up to 31 bytes, str8 (0xD9) for 32..=255, str16 (0xDA)
//     for 256..=65535, str32 (0xDB) above.  rmp_serde DOES emit str8.
//   - Binary: bin8 (0xC4) / bin16 (0xC5) / bin32 (0xC6).
//   - Arrays: fixarray up to 15, array16 (0xDC) for 16..=65535, array32
//     (0xDD) above.
//   - f32 = 0xCA, f64 = 0xCB.  rmp_serde DECODES f32/f64 leniently in both
//     directions (from_slice::<f32> accepts a 0xCB encoding and vice versa);
//     guest runtimes mirror this leniency (pinned below).
//   - Decoding a u64 REJECTS negative encodings (from_slice::<u64> on a
//     negfixint errors); pinned below.
//   - Gotcha: a Rust newtype struct wrapping a blob (`struct W(Vec<u8>)`)
//     serializes TRANSPARENTLY in rmp_serde (no array wrapper).  Newtype
//     wrappers are therefore deliberately absent from this primitive corpus;
//     their wire shape is pinned by the syscall corpus instead.
//
// Segment inventory (44 vectors):
//
//   0..=10   u64: 0, 1, 127, 128, 255, 256, 65535, 65536, 2^32-1, 2^32, 2^64-1
//   11..=23  i64: 1, 127, 128, -1, -32, -33, -128, -129, -32768, -32769,
//            -2^31, -2^31-1, i64::MIN
//   24..=25  f32: 1.5, 0.1
//   26..=27  f64: 1.5, -pi
//   28       nil
//   29..=30  bool: false, true
//   31..=36  str: len 0, 31, 32, 255, 256 (all-'a' fill), "héllo世界"
//   37..=39  bin: len 0, 255, 256 (byte i = i mod 251)
//   40..=42  array: len 0, 15, 16 (u64 elements 0..len-1)
//   43       combo: [u64 42, str "hi", bin 0xDEADBEEF]
//
// Regeneration (writes both fixture files; run once and commit):
//
//   cargo test -p testing --test compat_golden -- generate_mp_primitives_v1 --ignored
//
// Consumers:
//   - Rust:   this file, `mp_primitives_v1_roundtrip`
//   - C#:     `Mudu.Api.Tests/MpPrimitiveCorpusTests.cs`
//   - AS:     `crates/sdk/bindings/assemblyscript/run_mp_corpus_test.mjs`
// ---------------------------------------------------------------------------

/// Modulus of the bin fill pattern (matches the AS boundary test assets).
const MP_BIN_FILL_MOD: usize = 251;

/// The fixed content of the nested combo vector `[u64, str, bin]`.
const MP_COMBO_U64: u64 = 42;
const MP_COMBO_STR: &str = "hi";
const MP_COMBO_BIN: [u8; 4] = [0xDE, 0xAD, 0xBE, 0xEF];

/// One test vector of the primitive alignment corpus.
#[derive(Debug)]
enum MpVector {
    U64(u64),
    I64(i64),
    F32(f32),
    F64(f64),
    Nil,
    Bool(bool),
    Str(String),
    Bin(Vec<u8>),
    /// A u64 array with elements `0..len-1`.
    Array(u32),
    /// The nested `[u64, str, bin]` combo (`MP_COMBO_*` constants).
    Combo,
}

/// The 44 corpus vectors in segment order.
fn mp_vectors() -> Vec<MpVector> {
    let mut vectors = Vec::new();
    for v in [
        0u64,
        1,
        127,
        128,
        255,
        256,
        65535,
        65536,
        u32::MAX as u64,
        u32::MAX as u64 + 1,
        u64::MAX,
    ] {
        vectors.push(MpVector::U64(v));
    }
    for v in [
        1i64,
        127,
        128,
        -1,
        -32,
        -33,
        -128,
        -129,
        -32768,
        -32769,
        i32::MIN as i64,
        i32::MIN as i64 - 1,
        i64::MIN,
    ] {
        vectors.push(MpVector::I64(v));
    }
    vectors.push(MpVector::F32(1.5));
    vectors.push(MpVector::F32(0.1));
    vectors.push(MpVector::F64(1.5));
    vectors.push(MpVector::F64(-std::f64::consts::PI));
    vectors.push(MpVector::Nil);
    vectors.push(MpVector::Bool(false));
    vectors.push(MpVector::Bool(true));
    vectors.push(MpVector::Str(String::new()));
    for len in [31usize, 32, 255, 256] {
        vectors.push(MpVector::Str("a".repeat(len)));
    }
    vectors.push(MpVector::Str("héllo世界".to_string()));
    for len in [0usize, 255, 256] {
        vectors.push(MpVector::Bin(
            (0..len).map(|i| (i % MP_BIN_FILL_MOD) as u8).collect(),
        ));
    }
    for len in [0u32, 15, 16] {
        vectors.push(MpVector::Array(len));
    }
    vectors.push(MpVector::Combo);
    vectors
}

/// Borrowed byte-string wrapper that serializes as a MessagePack bin value
/// (mirrors `BinRef` in `mudu_binding::codec::syscall_payload`).
struct BinRef<'a>(&'a [u8]);

impl serde::Serialize for BinRef<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.0)
    }
}

/// Owned byte-string wrapper that deserializes a MessagePack bin value.
struct Bin(Vec<u8>);

struct BinVisitor;

impl<'de> serde::de::Visitor<'de> for BinVisitor {
    type Value = Bin;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a byte string")
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E> {
        Ok(Bin(value.to_vec()))
    }

    fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E> {
        Ok(Bin(value))
    }
}

impl<'de> serde::Deserialize<'de> for Bin {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_bytes(BinVisitor)
    }
}

/// Encodes one vector with rmp_serde exactly as the generator does.
fn mp_encode(vector: &MpVector) -> Vec<u8> {
    match vector {
        MpVector::U64(v) => rmp_serde::to_vec(v).unwrap(),
        MpVector::I64(v) => rmp_serde::to_vec(v).unwrap(),
        MpVector::F32(v) => rmp_serde::to_vec(v).unwrap(),
        MpVector::F64(v) => rmp_serde::to_vec(v).unwrap(),
        MpVector::Nil => rmp_serde::to_vec(&()).unwrap(),
        MpVector::Bool(v) => rmp_serde::to_vec(v).unwrap(),
        MpVector::Str(s) => rmp_serde::to_vec(s).unwrap(),
        MpVector::Bin(bytes) => rmp_serde::to_vec(&BinRef(bytes)).unwrap(),
        MpVector::Array(len) => {
            let elements: Vec<u64> = (0..*len as u64).collect();
            rmp_serde::to_vec(&elements).unwrap()
        }
        MpVector::Combo => {
            rmp_serde::to_vec(&(MP_COMBO_U64, MP_COMBO_STR, BinRef(&MP_COMBO_BIN))).unwrap()
        }
    }
}

/// JSON sidecar schema: one entry per segment.  `value` carries the literal
/// value (decimal string for ints/floats, JSON bool for bool, the string
/// itself for short strings); `len` marks pattern-generated payloads (see
/// `patterns` in the sidecar).
#[derive(Serialize)]
struct MpCorpusEntry {
    index: usize,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    len: Option<usize>,
}

#[derive(Serialize)]
struct MpCorpusPatterns {
    str_fill: &'static str,
    bin_fill: &'static str,
    array_fill: &'static str,
    combo: &'static str,
}

#[derive(Serialize)]
struct MpCorpusSidecar {
    format: &'static str,
    reference: &'static str,
    container: &'static str,
    regenerate: &'static str,
    patterns: MpCorpusPatterns,
    vectors: Vec<MpCorpusEntry>,
}

fn mp_corpus_entry(index: usize, vector: &MpVector) -> MpCorpusEntry {
    let (kind, value, len) = match vector {
        MpVector::U64(v) => ("u64", Some(v.to_string().into()), None),
        MpVector::I64(v) => ("i64", Some(v.to_string().into()), None),
        MpVector::F32(v) => ("f32", Some(v.to_string().into()), None),
        MpVector::F64(v) => ("f64", Some(v.to_string().into()), None),
        MpVector::Nil => ("nil", None, None),
        MpVector::Bool(v) => ("bool", Some((*v).into()), None),
        MpVector::Str(s) => {
            if !s.is_empty() && s.bytes().all(|b| b == b'a') {
                ("str", None, Some(s.len()))
            } else {
                ("str", Some(s.clone().into()), None)
            }
        }
        MpVector::Bin(bytes) => ("bin", None, Some(bytes.len())),
        MpVector::Array(n) => ("array", None, Some(*n as usize)),
        MpVector::Combo => ("combo", None, None),
    };
    MpCorpusEntry {
        index,
        kind,
        value,
        len,
    }
}

fn mp_corpus_sidecar(vectors: &[MpVector]) -> MpCorpusSidecar {
    MpCorpusSidecar {
        format: "mp-primitives-v1",
        reference: "rmp_serde 1.3.1 (rmp-serde =1.3.1, the repo's locked version)",
        container: "mp_primitives_v1.bin: one segment per vector, each a 4-byte big-endian u32 \
                    length followed by the rmp_serde MessagePack encoding of the vector; \
                    segment order matches the vectors array below",
        regenerate: "cargo test -p testing --test compat_golden -- generate_mp_primitives_v1 --ignored",
        patterns: MpCorpusPatterns {
            str_fill: "a str entry with len (no value) is that many ASCII 'a' bytes",
            bin_fill: "a bin entry with len has byte i = i mod 251",
            array_fill: "an array entry with len has u64 elements 0, 1, ..., len-1",
            combo: "the combo entry is the 3-array [u64 42, str \"hi\", bin 0xDEADBEEF]",
        },
        vectors: vectors
            .iter()
            .enumerate()
            .map(|(index, vector)| mp_corpus_entry(index, vector))
            .collect(),
    }
}

fn mp_corpus_sidecar_json(vectors: &[MpVector]) -> String {
    let mut json = serde_json::to_string_pretty(&mp_corpus_sidecar(vectors)).unwrap();
    json.push('\n');
    json
}

/// Generates the primitive alignment corpus.  Run once and commit the files:
///
///   cargo test -p testing --test compat_golden -- generate_mp_primitives_v1 --ignored
#[test]
#[ignore = "one-shot fixture generator"]
fn generate_mp_primitives_v1() {
    mudu_sys::fs::sync::sync_create_dir_all(FIXTURE_DIR).expect("create fixture directory");
    let vectors = mp_vectors();
    let frames: Vec<Vec<u8>> = vectors.iter().map(mp_encode).collect();
    mudu_sys::fs::sync::sync_write(
        fixture_path("mp_primitives_v1.bin"),
        pack_syscall_segments(&frames),
    )
    .unwrap();
    mudu_sys::fs::sync::sync_write(
        fixture_path("mp_primitives_v1.json"),
        mp_corpus_sidecar_json(&vectors),
    )
    .unwrap();
}

/// Asserts one segment decodes to the vector's value and re-encodes
/// byte-identically with rmp_serde.
fn mp_verify_segment(vector: &MpVector, segment: &[u8]) {
    assert_eq!(
        mp_encode(vector),
        segment,
        "re-encode mismatch for {vector:?}"
    );
    match vector {
        MpVector::U64(v) => assert_eq!(rmp_serde::from_slice::<u64>(segment).unwrap(), *v),
        MpVector::I64(v) => assert_eq!(rmp_serde::from_slice::<i64>(segment).unwrap(), *v),
        MpVector::F32(v) => assert_eq!(rmp_serde::from_slice::<f32>(segment).unwrap(), *v),
        MpVector::F64(v) => assert_eq!(rmp_serde::from_slice::<f64>(segment).unwrap(), *v),
        MpVector::Nil => {
            rmp_serde::from_slice::<()>(segment).unwrap();
        }
        MpVector::Bool(v) => assert_eq!(rmp_serde::from_slice::<bool>(segment).unwrap(), *v),
        MpVector::Str(s) => assert_eq!(rmp_serde::from_slice::<String>(segment).unwrap(), *s),
        MpVector::Bin(bytes) => {
            assert_eq!(rmp_serde::from_slice::<Bin>(segment).unwrap().0, *bytes)
        }
        MpVector::Array(len) => {
            let expected: Vec<u64> = (0..*len as u64).collect();
            assert_eq!(
                rmp_serde::from_slice::<Vec<u64>>(segment).unwrap(),
                expected
            );
        }
        MpVector::Combo => {
            let (n, s, b) = rmp_serde::from_slice::<(u64, String, Bin)>(segment).unwrap();
            assert_eq!(n, MP_COMBO_U64);
            assert_eq!(s, MP_COMBO_STR);
            assert_eq!(b.0, MP_COMBO_BIN);
        }
    }
}

/// Verifies that the committed primitive corpus matches the current rmp_serde
/// encoder byte for byte and that every segment decodes to its value.  Also
/// pins the decode-side rules guests must mirror: u64 rejects negative
/// encodings, and f32/f64 decode leniently across the 0xCA/0xCB markers.
#[test]
fn mp_primitives_v1_roundtrip() {
    let corpus_bytes = mudu_sys::fs::sync::sync_read_all(fixture_path("mp_primitives_v1.bin"))
        .expect("missing mp_primitives_v1.bin; run generate_mp_primitives_v1");
    let sidecar_json = mudu_sys::fs::sync::sync_read_all(fixture_path("mp_primitives_v1.json"))
        .expect("missing mp_primitives_v1.json; run generate_mp_primitives_v1");
    let vectors = mp_vectors();

    // The committed bytes and sidecar must match the current encoder exactly.
    let frames: Vec<Vec<u8>> = vectors.iter().map(mp_encode).collect();
    assert_eq!(corpus_bytes, pack_syscall_segments(&frames));
    assert_eq!(sidecar_json, mp_corpus_sidecar_json(&vectors).into_bytes());

    let segments = unpack_syscall_segments(&corpus_bytes);
    assert_eq!(segments.len(), vectors.len());
    for (vector, segment) in vectors.iter().zip(segments.iter()) {
        mp_verify_segment(vector, segment);
    }

    // Pinned decode rule: reading a u64 rejects negative encodings
    // (0xFF is the negfixint encoding of -1).
    assert!(rmp_serde::from_slice::<u64>(&[0xFF]).is_err());

    // Pinned decode rule: f32/f64 decode leniently across 0xCA/0xCB.
    let f64_bytes = rmp_serde::to_vec(&1.5f64).unwrap();
    assert_eq!(rmp_serde::from_slice::<f32>(&f64_bytes).unwrap(), 1.5f32);
    let f32_bytes = rmp_serde::to_vec(&1.5f32).unwrap();
    assert_eq!(rmp_serde::from_slice::<f64>(&f32_bytes).unwrap(), 1.5f64);
}
