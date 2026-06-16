use crate::common::codec::{Decode, Encode};
#[cfg(any(test, feature = "test"))]
use arbitrary::{Arbitrary, Unstructured};
use std::fmt::Debug;

pub fn _fuzz_decode_and_encode<'a, T: Arbitrary<'a> + Decode + Encode + Eq + Debug + 'static>(
    data: &'a [u8],
) {
    let mut u = Unstructured::new(data);
    loop {
        let _r = T::arbitrary(&mut u);
        let t = match _r {
            Ok(t) => t,
            Err(_e) => {
                break;
            }
        };
        let mut b = Vec::new();
        t.encode(&mut b).unwrap();
        let _size = t.size().unwrap();
        if _size != b.len() {
            let _ = t.size().unwrap();
        }

        assert_eq!(b.len(), _size);
        let _r = T::decode(&mut (b.clone(), 0));
        let _t = match _r {
            Ok(_t) => _t,
            Err(_e) => {
                panic!("{:?}", _e);
            }
        };
        assert_eq!(t.size().unwrap(), _t.size().unwrap());

        assert_eq!(t, _t);

        if u.is_empty() {
            break;
        }
    }
}
