//! Helpers that assert expected invariants.

/// Takes the value out of `opt`, leaving `None` behind.
///
/// # Panics
///
/// Panics if `opt` is `None`.
pub fn swap_some<T>(opt: &mut Option<T>) -> T {
    let mut o = None;
    std::mem::swap(&mut o, opt);
    match o {
        Some(t) => t,
        None => {
            panic!("expected some, but found none");
        }
    }
}
