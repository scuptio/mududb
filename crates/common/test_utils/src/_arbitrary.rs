//! Generic arbitrary data helpers.

use arbitrary::{Arbitrary, Unstructured};

/// Repeatedly generates arbitrary values from `data` until it is exhausted.
pub fn _arbitrary_data<'a, T: Arbitrary<'a> + 'static>(data: &'a [u8]) -> Vec<T> {
    let mut vec = vec![];
    let mut u = Unstructured::new(data);
    loop {
        let _r = T::arbitrary(&mut u);
        match _r {
            Ok(t) => {
                vec.push(t);
            }
            Err(_e) => {
                break;
            }
        };

        if u.is_empty() {
            break;
        }
    }
    vec
}

/// Generates exactly `n` arbitrary values.
pub fn _arbitrary_vec_n<'a, T: Arbitrary<'a> + 'static>(
    u: &mut Unstructured<'a>,
    n: usize,
) -> arbitrary::Result<Vec<T>> {
    let mut vec = vec![];
    for _i in 0..n {
        let t = T::arbitrary(u)?;
        vec.push(t);
    }
    Ok(vec)
}
