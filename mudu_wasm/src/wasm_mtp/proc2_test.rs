#[cfg(test)]
mod tests {
    use crate::wasm_mtp::proc2::object::Wallets;
    use crate::wasm_mtp::proc2::{proc_sys_call_mtp, proc2_mtp};
    use mududb::common::result::RS;
    use mududb::contract::database::entity::Entity;
    use mududb::error::ErrorCode;
    use mududb::types::dat_type_id::DatTypeID;
    use mududb::types::datum::{Datum, DatumDyn};
    use std::any::Any;

    #[test]
    fn proc2_mtp_computes_normal_case() -> RS<()> {
        let (sum, text) = proc2_mtp(10, 20, 10, "x".to_string())?;
        assert_eq!(sum, 30);
        assert_eq!(text, "xid:10, a=20, b=10, c=x");
        Ok(())
    }

    #[test]
    fn proc2_mtp_computes_negative_case() -> RS<()> {
        let (sum, text) = proc2_mtp(5, -15, 5, "neg".to_string())?;
        assert_eq!(sum, -10);
        assert_eq!(text, "xid:5, a=-15, b=5, c=neg");
        Ok(())
    }

    #[test]
    fn proc2_mtp_computes_zero_case() -> RS<()> {
        let (sum, text) = proc2_mtp(0, 0, 0, "zero".to_string())?;
        assert_eq!(sum, 0);
        assert_eq!(text, "xid:0, a=0, b=0, c=zero");
        Ok(())
    }

    #[test]
    fn wallets_entity_binary_roundtrip_and_field_access() -> RS<()> {
        let original = Wallets::new(Some(1), Some(100), Some(1_000_000));
        let binary = original.to_binary(&Wallets::dat_type())?;
        let restored = Wallets::from_binary(binary.as_ref())?;

        assert_eq!(*restored.get_user_id(), Some(1));
        assert_eq!(*restored.get_balance(), Some(100));
        assert_eq!(*restored.get_updated_at(), Some(1_000_000));

        // Verify individual field access through the entity API.
        assert_eq!(*restored.get_user_id(), *original.get_user_id());
        assert_eq!(*restored.get_balance(), *original.get_balance());
        assert_eq!(*restored.get_updated_at(), *original.get_updated_at());
        Ok(())
    }

    #[test]
    fn wallets_entity_textual_roundtrip() -> RS<()> {
        let original = Wallets::new(Some(7), Some(77), Some(777));
        let textual = original.to_textual(&Wallets::dat_type())?;
        let restored = Wallets::from_textual(&textual)?;
        assert_eq!(*restored.get_user_id(), Some(7));
        assert_eq!(*restored.get_balance(), Some(77));
        assert_eq!(*restored.get_updated_at(), Some(777));
        Ok(())
    }

    #[test]
    fn wallets_setters_and_new_empty() {
        let mut wallet = Wallets::new_empty();
        assert_eq!(*wallet.get_user_id(), None);
        assert_eq!(*wallet.get_balance(), None);
        assert_eq!(*wallet.get_updated_at(), None);

        wallet.set_user_id(1);
        wallet.set_balance(100);
        wallet.set_updated_at(1_000);

        assert_eq!(*wallet.get_user_id(), Some(1));
        assert_eq!(*wallet.get_balance(), Some(100));
        assert_eq!(*wallet.get_updated_at(), Some(1_000));
    }

    #[test]
    fn wallets_entity_metadata() -> RS<()> {
        assert_eq!(Wallets::object_name(), "wallets");
        assert_eq!(Wallets::tuple_desc().fields().len(), 3);

        let wallet = Wallets::new(Some(1), None, None);
        assert!(matches!(wallet.dat_type_id()?, DatTypeID::Record));
        Ok(())
    }

    #[test]
    fn wallets_clone_boxed_preserves_data() -> RS<()> {
        let wallet = Wallets::new(Some(2), Some(20), Some(200));
        let cloned = wallet.clone_boxed();
        let downcast = match (&*cloned as &dyn Any).downcast_ref::<Wallets>() {
            Some(v) => v,
            None => panic!("cloned datum should downcast to Wallets"),
        };
        assert_eq!(*downcast.get_user_id(), Some(2));
        assert_eq!(*downcast.get_balance(), Some(20));
        assert_eq!(*downcast.get_updated_at(), Some(200));
        Ok(())
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "opens a SQLite connection via FFI, which Miri cannot emulate"
    )]
    fn proc_sys_call_mtp_returns_error_without_session() {
        let result = proc_sys_call_mtp(1, 1, 2, "test".to_string());
        let err = match result {
            Ok(_) => panic!("expected an error"),
            Err(e) => e,
        };
        // Without a valid session the call fails. The exact error code depends on
        // the active sys_interface backend: the default stub returns
        // NotImplemented, while the standalone adapter returns EntityNotFound
        // because the session/context lookup fails.
        assert!(
            matches!(
                err.ec(),
                ErrorCode::NotImplemented | ErrorCode::EntityNotFound
            ),
            "expected NotImplemented or EntityNotFound, got {:?}",
            err.ec()
        );
    }

    #[test]
    fn wallets_entity_value_roundtrip() -> RS<()> {
        let original = Wallets::new(Some(3), Some(30), Some(300));
        let value = original.to_value(&Wallets::dat_type())?;
        let restored = Wallets::from_value(&value)?;
        assert_eq!(*restored.get_user_id(), Some(3));
        assert_eq!(*restored.get_balance(), Some(30));
        assert_eq!(*restored.get_updated_at(), Some(300));
        Ok(())
    }

    #[test]
    fn wallets_entity_field_binary_accessors() -> RS<()> {
        let wallet = Wallets::new(Some(42), Some(100), Some(1_000));
        let binary = match wallet.get_field_binary("user_id")? {
            Some(b) => b,
            None => panic!("user_id binary should be present"),
        };

        let mut empty = Wallets::new_empty();
        empty.set_field_binary("user_id", &binary)?;
        assert_eq!(*empty.get_user_id(), Some(42));
        Ok(())
    }

    #[test]
    fn wallets_entity_field_value_accessors() -> RS<()> {
        let wallet = Wallets::new(Some(42), Some(100), Some(1_000));
        let value = match wallet.get_field_value("user_id")? {
            Some(v) => v,
            None => panic!("user_id value should be present"),
        };

        let mut empty = Wallets::new_empty();
        empty.set_field_value("user_id", &value)?;
        assert_eq!(*empty.get_user_id(), Some(42));
        Ok(())
    }
}
