#[cfg(test)]
mod tests {
    use crate::backend::cfg_meta::ConfigMutability;

    #[test]
    fn as_str_returns_expected_labels() {
        assert_eq!(ConfigMutability::Persistent.as_str(), "persistent");
        assert_eq!(
            ConfigMutability::RestartRequired.as_str(),
            "restart-required"
        );
        assert_eq!(ConfigMutability::Runtime.as_str(), "runtime");
    }

    #[test]
    fn mutability_equality_and_copy() {
        let m = ConfigMutability::Runtime;
        let m2 = m;
        assert_eq!(m, ConfigMutability::Runtime);
        assert_eq!(m2, ConfigMutability::Runtime);
    }
}
