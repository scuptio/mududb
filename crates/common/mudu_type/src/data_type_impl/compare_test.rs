use crate::data_value::DataValue;
use crate::type_family::TypeFamily;
use arbitrary::Unstructured;
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

const SEED_COUNT: u64 = 32;
const SEED_BYTES_LEN: usize = 256;

fn comparable_type_ids() -> &'static [TypeFamily] {
    &[
        TypeFamily::I32,
        TypeFamily::I64,
        TypeFamily::String,
        TypeFamily::U128,
        TypeFamily::I128,
    ]
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

fn value_hash(id: TypeFamily, value: &DataValue) -> u64 {
    let mut hasher = DefaultHasher::new();
    id.fn_hash().unwrap()(value, &mut hasher).unwrap();
    hasher.finish()
}

#[test]
fn compare_functions_are_reflexive_and_hash_stable() {
    for &id in comparable_type_ids() {
        let order = id.fn_order().unwrap();
        let equal = id.fn_equal().unwrap();

        for seed in 0..SEED_COUNT {
            let bytes = seed_bytes(seed ^ ((id.to_u32() as u64) << 8), SEED_BYTES_LEN);
            let mut u = Unstructured::new(&bytes);
            let dt = id.fn_arb_param()(&mut u).unwrap();
            let value = match id.fn_arb_internal()(&mut u, &dt) {
                Ok(value) => value,
                Err(arbitrary::Error::NotEnoughData) => continue,
                Err(err) => panic!("arb value failed for {:?}: {:?}", id, err),
            };

            assert!(
                equal(&value, &value).unwrap(),
                "equal is not reflexive for {:?}",
                id
            );
            assert_eq!(order(&value, &value).unwrap(), Ordering::Equal);

            let h1 = value_hash(id, &value);
            let h2 = value_hash(id, &value);
            assert_eq!(h1, h2, "hash is unstable for {:?}", id);
        }
    }
}

#[test]
fn compare_functions_are_symmetric_and_consistent() {
    for &id in comparable_type_ids() {
        let order = id.fn_order().unwrap();
        let equal = id.fn_equal().unwrap();

        for seed in 0..SEED_COUNT {
            let left_bytes = seed_bytes(seed ^ ((id.to_u32() as u64) << 16), SEED_BYTES_LEN);
            let right_bytes = seed_bytes((seed + 1) ^ ((id.to_u32() as u64) << 24), SEED_BYTES_LEN);
            let mut left_u = Unstructured::new(&left_bytes);
            let mut right_u = Unstructured::new(&right_bytes);

            let left_dt = id.fn_arb_param()(&mut left_u).unwrap();
            let right_dt = id.fn_arb_param()(&mut right_u).unwrap();
            let left = match id.fn_arb_internal()(&mut left_u, &left_dt) {
                Ok(value) => value,
                Err(arbitrary::Error::NotEnoughData) => continue,
                Err(err) => panic!("left arb value failed for {:?}: {:?}", id, err),
            };
            let right = match id.fn_arb_internal()(&mut right_u, &right_dt) {
                Ok(value) => value,
                Err(arbitrary::Error::NotEnoughData) => continue,
                Err(err) => panic!("right arb value failed for {:?}: {:?}", id, err),
            };

            let left_right_equal = equal(&left, &right).unwrap();
            let right_left_equal = equal(&right, &left).unwrap();
            assert_eq!(
                left_right_equal, right_left_equal,
                "equal symmetry failed for {:?}",
                id
            );

            let left_right_order = order(&left, &right).unwrap();
            let right_left_order = order(&right, &left).unwrap();
            assert_eq!(
                left_right_order,
                right_left_order.reverse(),
                "order symmetry failed for {:?}",
                id
            );

            if left_right_equal {
                assert_eq!(left_right_order, Ordering::Equal);
                assert_eq!(value_hash(id, &left), value_hash(id, &right));
            }
        }
    }
}
