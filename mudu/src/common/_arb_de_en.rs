use crate::common::bc::{hdr_size, tail_size};
use crate::common::bc_dec::{decode_binary, Decode};
use crate::common::bc_enc::{encode_binary, Encode};
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
        let _r = encode_binary(&t);
        let b = match _r {
            Ok(b) => b,
            Err(_e) => {
                panic!("{:?}", _e);
            }
        };
        let _size = t.size().unwrap();
        if _size != b.len() {
            let _ = t.size().unwrap();
        }

        assert_eq!(b.len(), _size + hdr_size() + tail_size());
        let _r = decode_binary::<T>(&b);
        let _t = match _r {
            Ok(_t) => _t,
            Err(_e) => {
                panic!("{:?}", _e);
            }
        };
        assert_eq!(t.size().unwrap(), _t.size().unwrap());

        assert_eq!(t, _t);

        if u.len() == 0 {
            break;
        }
    }
}
