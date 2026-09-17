#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::common::xid::{INVALID_OID, is_xid_invalid, xid_from_binary, xid_to_binary};

    #[test]
    fn test_is_xid_invalid() {
        assert!(is_xid_invalid(&INVALID_OID));
        assert!(!is_xid_invalid(&1));
        assert!(!is_xid_invalid(&u128::MAX));
    }

    #[test]
    fn test_xid_to_binary_and_from_binary_round_trip() {
        for xid in [INVALID_OID, 1, 0x1234_5678_9ABC_DEF0_u128, u128::MAX] {
            let binary = xid_to_binary(xid);
            assert_eq!(binary.len(), 16);
            let decoded = xid_from_binary(&binary).unwrap();
            assert_eq!(decoded, xid);
        }
    }

    #[test]
    fn test_xid_from_binary_known_value() {
        // Network (big) endian: u128 1 is fifteen zero bytes followed by 0x01.
        let mut binary = [0u8; 16];
        binary[15] = 1;
        assert_eq!(xid_from_binary(&binary).unwrap(), 1);

        let mut binary = [0u8; 16];
        binary[0] = 1;
        assert_eq!(xid_from_binary(&binary).unwrap(), 1 << 120);
    }

    #[test]
    fn test_xid_from_binary_too_short() {
        assert!(xid_from_binary(&[]).is_err());
        assert!(xid_from_binary(&[0u8; 15]).is_err());
    }
}
