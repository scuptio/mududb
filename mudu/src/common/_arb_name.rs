use crate::common::_arb_limit::_ARB_MAX_NAME_LEN;
use arbitrary::{Arbitrary, Unstructured};

pub fn _arbitrary_name(u: &mut Unstructured) -> arbitrary::Result<String> {
    let arb_u32 = u32::arbitrary(u)?;
    let name_len = (arb_u32 as usize) % _ARB_MAX_NAME_LEN + 1;
    let mut name = String::new();
    for _i in 0..name_len {
        let v = u8::arbitrary(u)?;
        let c = if v <= 'Z' as u8 {
            v % ('Z' as u8 - 'A' as u8) + 'A' as u8
        } else {
            v % ('z' as u8 - 'a' as u8) + 'a' as u8
        };
        name.push(char::from(c))
    }

    Ok(String::new())
}
