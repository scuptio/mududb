use mudu::common::id::OID;
use mudu::error::ec::EC;
use mudu::error::err::MError;
use mudu::m_error;

use crate::universal::uni_error::UniError;
use crate::universal::uni_oid::UniOid;

pub fn oid_from_mu(mu_oid: UniOid) -> OID {
    mu_oid.to_oid()
}

pub fn oid_to_mu(oid: OID) -> UniOid {
    UniOid::from(oid)
}

pub fn error_to_mu(error: MError) -> UniError {
    UniError {
        err_code: error.ec().to_u32(),
        err_msg: error.message().to_string(),
    }
}

pub fn error_from_mu(error: UniError) -> MError {
    let error_code = EC::from_u32(error.err_code).unwrap_or(EC::InternalErr);
    m_error!(error_code, error.err_msg.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test constant definitions
    const TEST_HIGH: u64 = 0x1234567890ABCDEF;
    const TEST_LOW: u64 = 0xFEDCBA0987654321;
    const FULL_OID: u128 = 0x1234567890ABCDEFFEDCBA0987654321;

    #[test]
    fn test_oid_to_mu_basic() {
        // Test basic conversion from u128 to MuOid
        let oid = FULL_OID;
        let mu_oid = oid_to_mu(oid);

        assert_eq!(mu_oid.h, TEST_HIGH);
        assert_eq!(mu_oid.l, TEST_LOW);
    }

    #[test]
    fn test_oid_from_mu_basic() {
        // Test basic conversion from UniOid to u128
        let mu_oid = UniOid {
            h: TEST_HIGH,
            l: TEST_LOW,
        };
        let oid = oid_from_mu(mu_oid);

        assert_eq!(oid, FULL_OID);
    }

    #[test]
    fn test_oid_to_mu_and_back() {
        // Test round-trip conversion: u128 -> MuOid -> u128
        let original_oid: u128 = 0xDEADBEEF123456789876543210ABCDEF;
        let mu_oid = oid_to_mu(original_oid);
        let restored_oid = oid_from_mu(mu_oid);

        assert_eq!(original_oid, restored_oid);
    }

    #[test]
    fn test_oid_from_mu_and_back() {
        // Test round-trip conversion: UniOid -> u128 -> MuOid
        let original_mu = UniOid {
            h: 0xAABBCCDDEEFF0011,
            l: 0x1122334455667788,
        };
        let oid = oid_from_mu(original_mu.clone());
        let restored_mu = oid_to_mu(oid);

        assert_eq!(original_mu.h, restored_mu.h);
        assert_eq!(original_mu.l, restored_mu.l);
    }

    #[test]
    fn test_edge_cases() {
        // Test maximum value
        let max_oid = u128::MAX;
        let mu_max = oid_to_mu(max_oid);
        assert_eq!(mu_max.h, u64::MAX);
        assert_eq!(mu_max.l, u64::MAX);
        assert_eq!(oid_from_mu(mu_max), max_oid);

        // Test minimum value
        let min_oid = 0u128;
        let mu_min = oid_to_mu(min_oid);
        assert_eq!(mu_min.h, 0);
        assert_eq!(mu_min.l, 0);
        assert_eq!(oid_from_mu(mu_min), min_oid);

        // Test only high bits set
        let high_only_oid = (TEST_HIGH as u128) << 64;
        let mu_high = oid_to_mu(high_only_oid);
        assert_eq!(mu_high.h, TEST_HIGH);
        assert_eq!(mu_high.l, 0);
        assert_eq!(oid_from_mu(mu_high), high_only_oid);

        // Test only low bits set
        let low_only_oid = TEST_LOW as u128;
        let mu_low = oid_to_mu(low_only_oid);
        assert_eq!(mu_low.h, 0);
        assert_eq!(mu_low.l, TEST_LOW);
        assert_eq!(oid_from_mu(mu_low), low_only_oid);
    }

    #[test]
    fn test_specific_patterns() {
        // Test specific bit patterns that might reveal edge cases
        let test_cases = vec![
            // (u128_value, (expected_high_bits, expected_low_bits))
            (
                0x00000000000000000000000000000000,
                (0x0000000000000000, 0x0000000000000000),
            ),
            (
                0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF,
                (0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF),
            ),
            (
                0x80000000000000000000000000000000,
                (0x8000000000000000, 0x0000000000000000),
            ),
            (
                0x00000000000000008000000000000000,
                (0x0000000000000000, 0x8000000000000000),
            ),
            // Alternating bit patterns
            (
                0x5555555555555555AAAAAAAAAAAAAAAA,
                (0x5555555555555555, 0xAAAAAAAAAAAAAAAA),
            ),
            (
                0xAAAAAAAAAAAAAAAA5555555555555555,
                (0xAAAAAAAAAAAAAAAA, 0x5555555555555555),
            ),
        ];

        for (oid, (expected_high, expected_low)) in test_cases {
            let mu_oid = oid_to_mu(oid);
            assert_eq!(
                mu_oid.h, expected_high,
                "High bits mismatch for {:032X}",
                oid
            );
            assert_eq!(mu_oid.l, expected_low, "Low bits mismatch for {:032X}", oid);

            let restored = oid_from_mu(mu_oid);
            assert_eq!(restored, oid, "Restoration mismatch for {:032X}", oid);
        }
    }

    #[test]
    fn test_shift_operations() {
        // Verify that the shift operations in oid_from_mu are correct
        let mu_oid = UniOid {
            h: 0x12345678,
            l: 0x9ABCDEF0,
        };
        let oid = oid_from_mu(mu_oid.clone());

        // Manual calculation for verification
        let manual_result = ((mu_oid.h as u128) << 64) | (mu_oid.l as u128);
        assert_eq!(oid, manual_result);
    }

    #[test]
    fn test_bitmask_operations() {
        // Verify that the bitmask in oid_to_mu correctly extracts low bits
        let test_values = vec![
            u128::MAX,
            0x1234567890ABCDEFFEDCBA0987654321,
            0x0000000000000000FFFFFFFFFFFFFFFF,
        ];

        for oid in test_values {
            let mu_oid = oid_to_mu(oid);
            let mask = (1 << 64) - 1; // 64-bit mask: 0xFFFFFFFFFFFFFFFF

            // Verify low bits extraction
            let expected_low = (oid & mask) as u64;
            assert_eq!(
                mu_oid.l, expected_low,
                "Low bits extraction failed for {:032X}",
                oid
            );

            // Verify high bits extraction
            let expected_high = (oid >> 64) as u64;
            assert_eq!(
                mu_oid.h, expected_high,
                "High bits extraction failed for {:032X}",
                oid
            );
        }
    }
}
