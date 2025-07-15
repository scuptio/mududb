use arbitrary::{Arbitrary, Unstructured};

pub fn _arbitrary_string(u: &mut Unstructured, len: usize) -> arbitrary::Result<String> {
    if len == 0 {
        Ok(String::new())
    } else {
        let v = u32::arbitrary(u)?;
        let str_len = (v as usize) % len;
        let d = u.bytes(str_len)?;
        let uu = Unstructured::new(d);
        let name = String::arbitrary_take_rest(uu)?;
        Ok(name)
    }
}
