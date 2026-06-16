use crate::_arb_limit;
use arbitrary::{Arbitrary, Unstructured};

pub fn _arbitrary_name(u: &mut Unstructured) -> arbitrary::Result<String> {
    let arb_u32 = u32::arbitrary(u)?;
    let name_len = (arb_u32 as usize) % _arb_limit::_ARB_MAX_NAME_LEN + 1;
    let mut name = String::new();
    for _i in 0..name_len {
        let v = u8::arbitrary(u)?;
        let c = if v <= b'Z' {
            v % (b'Z' - b'A') + b'A'
        } else {
            v % (b'z' - b'a') + b'a'
        };
        name.push(char::from(c))
    }

    Ok(String::new())
}
