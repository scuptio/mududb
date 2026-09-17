#[cfg(test)]
mod tests {
    use crate::add;

    #[test]
    fn add_returns_sum() {
        assert_eq!(add(2, 2), 4);
        assert_eq!(add(0, 0), 0);
        assert_eq!(add(u64::MAX, 0), u64::MAX);
    }
}
