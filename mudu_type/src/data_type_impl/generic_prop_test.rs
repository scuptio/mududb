use crate::data_type::DataType;
use crate::datum::DatumDyn;
use crate::type_family::TypeFamily;
use arbitrary::Unstructured;

const SEED_COUNT: u64 = 32;
const SEED_BYTES_LEN: usize = 512;

fn supported_scalar_type_ids() -> &'static [TypeFamily] {
    &[
        TypeFamily::I32,
        TypeFamily::I64,
        TypeFamily::F32,
        TypeFamily::F64,
        TypeFamily::String,
        TypeFamily::U128,
        TypeFamily::I128,
        TypeFamily::Binary,
    ]
}

fn supported_complex_type_ids() -> &'static [TypeFamily] {
    &[TypeFamily::Array, TypeFamily::Record]
}

fn seed_bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.push((state & 0xff) as u8);
    }
    out
}

fn canonical_binary(
    id: TypeFamily,
    dt: &DataType,
    value: &crate::data_value::DataValue,
) -> Vec<u8> {
    id.fn_send()(value, dt).unwrap().as_ref().to_vec()
}

fn assert_binary_roundtrip(id: TypeFamily, dt: &DataType, value: &crate::data_value::DataValue) {
    let binary = id.fn_send()(value, dt).unwrap();
    let (decoded, used) = id.fn_recv()(binary.as_ref(), dt).unwrap();
    assert_eq!(
        used as usize,
        binary.as_ref().len(),
        "binary recv size mismatch for {:?}",
        id
    );
    assert_eq!(
        canonical_binary(id, dt, &decoded),
        binary.as_ref(),
        "binary roundtrip mismatch for {:?}",
        id
    );

    let mut buf = vec![0u8; binary.as_ref().len()];
    let sent = id.fn_send_to()(value, dt, &mut buf).unwrap();
    assert_eq!(
        sent as usize,
        binary.as_ref().len(),
        "send_to len mismatch for {:?}",
        id
    );
    assert_eq!(buf, binary.as_ref(), "send_to bytes mismatch for {:?}", id);
}

fn assert_textual_roundtrip(id: TypeFamily, dt: &DataType, value: &crate::data_value::DataValue) {
    let textual = id.fn_output()(value, dt).unwrap().into();
    let decoded = id.fn_input()(&textual, dt).unwrap();
    assert_eq!(
        decoded.type_family().unwrap(),
        id,
        "textual parse type mismatch for {:?}",
        id
    );
    let textual2 = id.fn_output()(&decoded, dt).unwrap().into();
    let decoded2 = id.fn_input()(&textual2, dt).unwrap();
    assert_eq!(
        decoded2.type_family().unwrap(),
        id,
        "textual reparse type mismatch for {:?}",
        id
    );
}

fn assert_json_roundtrip(id: TypeFamily, dt: &DataType, value: &crate::data_value::DataValue) {
    let json = id.fn_output_json()(value, dt).unwrap().into_json_value();
    let decoded = id.fn_input_json()(&json, dt).unwrap();
    assert_eq!(
        decoded.type_family().unwrap(),
        id,
        "json parse type mismatch for {:?}",
        id
    );
    let json2 = id.fn_output_json()(&decoded, dt).unwrap().into_json_value();
    let decoded2 = id.fn_input_json()(&json2, dt).unwrap();
    assert_eq!(
        decoded2.type_family().unwrap(),
        id,
        "json reparse type mismatch for {:?}",
        id
    );
}

fn assert_msgpack_roundtrip(id: TypeFamily, dt: &DataType, value: &crate::data_value::DataValue) {
    let msgpack = id.fn_output_msg_pack()(value, dt).unwrap();
    let decoded = id.fn_input_msg_pack()(&msgpack, dt).unwrap();
    assert_eq!(
        decoded.type_family().unwrap(),
        id,
        "msgpack parse type mismatch for {:?}",
        id
    );
    let msgpack2 = id.fn_output_msg_pack()(&decoded, dt).unwrap();
    let decoded2 = id.fn_input_msg_pack()(&msgpack2, dt).unwrap();
    assert_eq!(
        decoded2.type_family().unwrap(),
        id,
        "msgpack reparse type mismatch for {:?}",
        id
    );
}

fn assert_default_is_sendable(id: TypeFamily, dt: &DataType) {
    let value = id.fn_default()(dt).unwrap();
    assert_eq!(
        value.type_family().unwrap(),
        id,
        "default type mismatch for {:?}",
        id
    );

    let binary = id.fn_send()(&value, dt).unwrap();
    let len = id.fn_send_data_len()(&value, dt).unwrap();
    assert_eq!(
        binary.as_ref().len(),
        len as usize,
        "default data len mismatch for {:?}",
        id
    );

    if let Some(type_len) = id.fn_send_type_len()(dt).unwrap() {
        assert_eq!(
            binary.as_ref().len(),
            type_len as usize,
            "default fixed len mismatch for {:?}",
            id
        );
    }
}

#[test]
fn supported_scalar_arbitrary_values_cover_roundtrip_paths() {
    for &id in supported_scalar_type_ids() {
        for seed in 0..SEED_COUNT {
            let bytes = seed_bytes(seed ^ id.to_u32() as u64, SEED_BYTES_LEN);
            let mut u = Unstructured::new(&bytes);
            let dt = id.fn_arb_param()(&mut u).unwrap();
            assert_eq!(dt.type_family(), id, "arb param type mismatch for {:?}", id);

            let value = match id.fn_arb_internal()(&mut u, &dt) {
                Ok(value) => value,
                Err(arbitrary::Error::NotEnoughData) => continue,
                Err(err) => panic!("arb value failed for {:?}: {:?}", id, err),
            };
            assert_eq!(
                value.type_family().unwrap(),
                id,
                "arb value type mismatch for {:?}",
                id
            );

            assert_binary_roundtrip(id, &dt, &value);
            assert_textual_roundtrip(id, &dt, &value);
            assert_json_roundtrip(id, &dt, &value);
            assert_msgpack_roundtrip(id, &dt, &value);
        }
    }
}

#[test]
fn supported_scalar_printable_values_parse_back() {
    for &id in supported_scalar_type_ids() {
        for seed in 0..SEED_COUNT {
            let bytes = seed_bytes((seed << 8) ^ id.to_u32() as u64, SEED_BYTES_LEN);
            let mut u = Unstructured::new(&bytes);
            let dt = id.fn_arb_param()(&mut u).unwrap();
            let printable = match id.fn_arb_printable()(&mut u, &dt) {
                Ok(printable) => printable,
                Err(arbitrary::Error::NotEnoughData) => continue,
                Err(err) => panic!("arb printable failed for {:?}: {:?}", id, err),
            };
            let value = id.fn_input()(&printable, &dt).unwrap();
            assert_eq!(
                value.type_family().unwrap(),
                id,
                "printable parse type mismatch for {:?}",
                id
            );
            assert_textual_roundtrip(id, &dt, &value);
        }
    }
}

#[test]
fn supported_scalar_default_values_are_sendable() {
    for &id in supported_scalar_type_ids() {
        for seed in 0..SEED_COUNT {
            let bytes = seed_bytes((seed << 16) ^ id.to_u32() as u64, SEED_BYTES_LEN);
            let mut u = Unstructured::new(&bytes);
            let dt = id.fn_arb_param()(&mut u).unwrap();
            assert_default_is_sendable(id, &dt);
        }
    }
}

#[test]
fn supported_complex_values_cover_roundtrip_paths() {
    for &id in supported_complex_type_ids() {
        for seed in 0..SEED_COUNT {
            let bytes = seed_bytes(seed ^ (id.to_u32() as u64) << 24, SEED_BYTES_LEN);
            let mut u = Unstructured::new(&bytes);
            let dt = id.fn_arb_param()(&mut u).unwrap();
            let value = match id.fn_arb_internal()(&mut u, &dt) {
                Ok(value) => value,
                Err(arbitrary::Error::NotEnoughData) => continue,
                Err(err) => panic!("complex arb value failed for {:?}: {:?}", id, err),
            };
            assert_eq!(
                value.type_family().unwrap(),
                id,
                "complex arb value type mismatch for {:?}",
                id
            );

            assert_binary_roundtrip(id, &dt, &value);
            assert_textual_roundtrip(id, &dt, &value);
            assert_json_roundtrip(id, &dt, &value);
            assert_msgpack_roundtrip(id, &dt, &value);
            assert_default_is_sendable(id, &dt);
        }
    }
}

#[test]
fn supported_complex_printable_values_parse_back() {
    for &id in supported_complex_type_ids() {
        for seed in 0..SEED_COUNT {
            let bytes = seed_bytes((seed << 32) ^ id.to_u32() as u64, SEED_BYTES_LEN);
            let mut u = Unstructured::new(&bytes);
            let dt = id.fn_arb_param()(&mut u).unwrap();
            let printable = match id.fn_arb_printable()(&mut u, &dt) {
                Ok(printable) => printable,
                Err(arbitrary::Error::NotEnoughData) => continue,
                Err(err) => panic!("complex arb printable failed for {:?}: {:?}", id, err),
            };
            let value = id.fn_input()(&printable, &dt).unwrap();
            assert_eq!(
                value.type_family().unwrap(),
                id,
                "complex printable parse type mismatch for {:?}",
                id
            );
            assert_textual_roundtrip(id, &dt, &value);
        }
    }
}
