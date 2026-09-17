#[cfg(test)]
mod tests {
    use crate::compat::install_compatibility_router;

    #[test]
    fn install_compatibility_router_is_idempotent() {
        install_compatibility_router();
        install_compatibility_router();
        assert!(mudu_compat_migrate::global::is_installed());
    }
}
