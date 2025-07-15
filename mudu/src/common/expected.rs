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
