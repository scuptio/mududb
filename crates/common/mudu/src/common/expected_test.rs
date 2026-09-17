#[cfg(test)]
mod tests {
    use crate::common::expected::swap_some;

    #[test]
    fn swap_some_returns_value_and_leaves_none() {
        let mut opt = Some(42);
        let value = swap_some(&mut opt);
        assert_eq!(value, 42);
        assert!(opt.is_none());
    }

    #[test]
    #[should_panic(expected = "expected some, but found none")]
    fn swap_some_panics_on_none() {
        let mut opt: Option<i32> = None;
        swap_some(&mut opt);
    }
}
