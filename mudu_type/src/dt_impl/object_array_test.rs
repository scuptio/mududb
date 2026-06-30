use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dat_value::DatValue;
use crate::dt_impl::dt_create::{create_array_type, create_object_type, create_string_type};
use arbitrary::Unstructured;

fn seeded_bytes(seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut bytes = Vec::with_capacity(256);
    for _ in 0..256 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        bytes.push((state & 0xff) as u8);
    }
    bytes
}

fn assert_binary_roundtrip(id: DatTypeID, dt: &DatType, value: &DatValue) {
    let binary = id.fn_send()(value, dt).unwrap();
    let (decoded, used) = id.fn_recv()(binary.as_ref(), dt).unwrap();
    assert_eq!(used as usize, binary.as_ref().len());
    let binary2 = id.fn_send()(&decoded, dt).unwrap();
    assert_eq!(binary.as_ref(), binary2.as_ref());
}

#[test]
fn array_arb_param_produces_supported_inner_type() {
    for seed in 0..32 {
        let bytes = seeded_bytes(seed);
        let mut u = Unstructured::new(&bytes);
        let dt = DatTypeID::Array.fn_arb_param()(&mut u).unwrap();
        assert_eq!(dt.dat_type_id(), DatTypeID::Array);
        let inner = dt.expect_array_param().dat_type();
        assert!(matches!(
            inner.dat_type_id(),
            DatTypeID::I32
                | DatTypeID::I64
                | DatTypeID::F32
                | DatTypeID::F64
                | DatTypeID::String
                | DatTypeID::U128
                | DatTypeID::I128
                | DatTypeID::Binary
        ));
    }
}

#[test]
fn array_roundtrip_with_variable_width_inner_type() {
    let dt = create_array_type(create_string_type(Some(12)));
    let value = DatValue::from_array(vec![
        DatValue::from_string("alpha".to_string()),
        DatValue::from_string(String::new()),
        DatValue::from_string("zeta".to_string()),
    ]);

    assert_binary_roundtrip(DatTypeID::Array, &dt, &value);

    let textual = DatTypeID::Array.fn_output()(&value, &dt).unwrap();
    let parsed = DatTypeID::Array.fn_input()(textual.as_ref(), &dt).unwrap();
    assert_eq!(
        DatTypeID::Array.fn_send()(&parsed, &dt).unwrap().as_ref(),
        DatTypeID::Array.fn_send()(&value, &dt).unwrap().as_ref()
    );
}

#[test]
fn object_arb_param_produces_named_fields() {
    for seed in 100..132 {
        let bytes = seeded_bytes(seed);
        let mut u = Unstructured::new(&bytes);
        let dt = DatTypeID::Record.fn_arb_param()(&mut u).unwrap();
        let record = dt.expect_record_param();
        assert_eq!(dt.dat_type_id(), DatTypeID::Record);
        assert!(!record.record_name().is_empty());
        assert!(!record.fields().is_empty());
        for (name, field_ty) in record.fields() {
            assert!(!name.is_empty());
            assert!(matches!(
                field_ty.dat_type_id(),
                DatTypeID::I32
                    | DatTypeID::I64
                    | DatTypeID::F32
                    | DatTypeID::F64
                    | DatTypeID::String
                    | DatTypeID::U128
                    | DatTypeID::I128
                    | DatTypeID::Binary
                    | DatTypeID::Array
            ));
        }
    }
}

#[test]
fn object_roundtrip_with_nested_array_field() {
    let score_type = create_array_type(DatType::default_for(DatTypeID::I32));
    let dt = create_object_type(
        "player".to_string(),
        vec![
            ("name".to_string(), create_string_type(Some(16))),
            ("scores".to_string(), score_type.clone()),
            ("blob".to_string(), DatType::new_no_param(DatTypeID::Binary)),
        ],
    );
    let value = DatValue::from_record(vec![
        DatValue::from_string("neo".to_string()),
        DatValue::from_array(vec![
            DatValue::from_i32(7),
            DatValue::from_i32(11),
            DatValue::from_i32(-3),
        ]),
        DatValue::from_binary(vec![1, 2, 3, 5, 8]),
    ]);

    assert_binary_roundtrip(DatTypeID::Record, &dt, &value);

    let json = DatTypeID::Record.fn_output_json()(&value, &dt).unwrap();
    let parsed = DatTypeID::Record.fn_input_json()(&json.into_json_value(), &dt).unwrap();
    assert_eq!(
        DatTypeID::Record.fn_send()(&parsed, &dt).unwrap().as_ref(),
        DatTypeID::Record.fn_send()(&value, &dt).unwrap().as_ref()
    );
}

#[test]
fn object_arbitrary_value_matches_generated_schema() {
    for seed in 200..216 {
        let bytes = seeded_bytes(seed);
        let mut u = Unstructured::new(&bytes);
        let dt = DatTypeID::Record.fn_arb_param()(&mut u).unwrap();
        let value = match DatTypeID::Record.fn_arb_internal()(&mut u, &dt) {
            Ok(value) => value,
            Err(arbitrary::Error::NotEnoughData) => continue,
            Err(err) => panic!("unexpected arbitrary error: {:?}", err),
        };
        let record = value.expect_record();
        assert_eq!(record.len(), dt.expect_record_param().fields().len());
        assert_binary_roundtrip(DatTypeID::Record, &dt, &value);
    }
}
