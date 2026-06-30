//! Shared newtype macro for 64-bit identifier types.
//!
//! `define_u64_id!` wraps a `u64` in a strongly typed struct and provides the
//! common operations needed by identifiers such as [`crate::wal::lsn::LSN`] and
//! [`crate::storage::page::PageId`].

/// Defines a `u64` newtype with integer-like arithmetic and conversions.
#[macro_export]
macro_rules! define_u64_id {
    ($(#[$meta:meta])* $vis:vis struct $name:ident) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash,
        )]
        $vis struct $name(u64);

        impl $name {
            /// Maximum value.
            pub const MAX: Self = Self(u64::MAX);
            /// Minimum value.
            pub const MIN: Self = Self(u64::MIN);
            /// Sentinel value used to mark an unset/invalid identifier.
            ///
            /// This is the same as [`Self::MAX`].
            pub const INVALID: Self = Self(u64::MAX);

            /// Constructs the identifier from a raw `u64`.
            #[inline]
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            /// Returns the invalid sentinel value.
            #[inline]
            pub const fn invalid() -> Self {
                Self::INVALID
            }

            /// Marks this identifier as invalid.
            #[inline]
            pub fn set_invalid(&mut self) {
                self.0 = u64::MAX;
            }

            /// Returns `true` if the identifier is not the invalid sentinel.
            #[inline]
            pub const fn is_valid(self) -> bool {
                self.0 != u64::MAX
            }

            /// Returns the raw `u64` value.
            #[inline]
            pub const fn get(self) -> u64 {
                self.0
            }

            /// Returns the value as `u64`.
            #[inline]
            pub const fn as_u64(self) -> u64 {
                self.0
            }

            /// Returns the value as `usize`.
            #[inline]
            pub fn as_usize(self) -> usize {
                self.0 as usize
            }

            /// Saturating integer addition.
            #[inline]
            pub const fn saturating_add(self, rhs: u64) -> Self {
                Self(self.0.saturating_add(rhs))
            }

            /// Saturating integer subtraction.
            #[inline]
            pub const fn saturating_sub(self, rhs: u64) -> Self {
                Self(self.0.saturating_sub(rhs))
            }

            /// Checked integer addition.
            #[inline]
            pub const fn checked_add(self, rhs: u64) -> Option<Self> {
                match self.0.checked_add(rhs) {
                    Some(v) => Some(Self(v)),
                    None => None,
                }
            }

            /// Checked integer subtraction.
            #[inline]
            pub const fn checked_sub(self, rhs: u64) -> Option<Self> {
                match self.0.checked_sub(rhs) {
                    Some(v) => Some(Self(v)),
                    None => None,
                }
            }

            /// Checked integer multiplication.
            #[inline]
            pub const fn checked_mul(self, rhs: u64) -> Option<Self> {
                match self.0.checked_mul(rhs) {
                    Some(v) => Some(Self(v)),
                    None => None,
                }
            }
        }

        impl ::std::fmt::Display for $name {
            #[inline]
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<u64> for $name {
            #[inline]
            fn from(value: u64) -> Self {
                Self(value)
            }
        }

        impl From<$name> for u64 {
            #[inline]
            fn from(value: $name) -> u64 {
                value.0
            }
        }

        impl From<usize> for $name {
            #[inline]
            fn from(value: usize) -> Self {
                Self(value as u64)
            }
        }

        impl From<$name> for usize {
            #[inline]
            fn from(value: $name) -> usize {
                value.0 as usize
            }
        }

        impl PartialEq<u64> for $name {
            #[inline]
            fn eq(&self, other: &u64) -> bool {
                self.0 == *other
            }
        }

        impl PartialOrd<u64> for $name {
            #[inline]
            fn partial_cmp(&self, other: &u64) -> Option<::std::cmp::Ordering> {
                self.0.partial_cmp(other)
            }
        }

        impl ::std::ops::Add<u64> for $name {
            type Output = Self;
            #[inline]
            fn add(self, rhs: u64) -> Self::Output {
                Self(self.0 + rhs)
            }
        }

        impl ::std::ops::Sub<u64> for $name {
            type Output = Self;
            #[inline]
            fn sub(self, rhs: u64) -> Self::Output {
                Self(self.0 - rhs)
            }
        }

        impl ::std::ops::Add<$name> for $name {
            type Output = Self;
            #[inline]
            fn add(self, rhs: $name) -> Self::Output {
                Self(self.0 + rhs.0)
            }
        }

        impl ::std::ops::Sub<$name> for $name {
            type Output = Self;
            #[inline]
            fn sub(self, rhs: $name) -> Self::Output {
                Self(self.0 - rhs.0)
            }
        }

        impl ::std::ops::AddAssign<u64> for $name {
            #[inline]
            fn add_assign(&mut self, rhs: u64) {
                self.0 += rhs;
            }
        }

        impl ::std::ops::SubAssign<u64> for $name {
            #[inline]
            fn sub_assign(&mut self, rhs: u64) {
                self.0 -= rhs;
            }
        }

        impl ::std::ops::AddAssign<$name> for $name {
            #[inline]
            fn add_assign(&mut self, rhs: $name) {
                self.0 += rhs.0;
            }
        }

        impl ::std::ops::SubAssign<$name> for $name {
            #[inline]
            fn sub_assign(&mut self, rhs: $name) {
                self.0 -= rhs.0;
            }
        }

        impl ::serde::Serialize for $name {
            #[inline]
            fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.0.serialize(serializer)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            #[inline]
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                u64::deserialize(deserializer).map(Self)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::assertions_on_constants
    )]

    crate::define_u64_id!(struct TestId);

    #[test]
    fn constants() {
        assert_eq!(TestId::MAX, TestId::INVALID);
        assert_eq!(TestId::MAX.get(), u64::MAX);
        assert_eq!(TestId::MIN.get(), u64::MIN);
        assert_eq!(TestId::INVALID.get(), u64::MAX);
    }

    #[test]
    fn new_get_as_u64() {
        let id = TestId::new(42);
        assert_eq!(id.get(), 42);
        assert_eq!(id.as_u64(), 42);
    }

    #[test]
    fn invalid_is_valid_set_invalid() {
        let mut id = TestId::new(1);
        assert!(id.is_valid());
        id.set_invalid();
        assert!(!id.is_valid());
        assert_eq!(id, TestId::INVALID);
        assert_eq!(id, TestId::invalid());
    }

    #[test]
    fn as_usize() {
        let id = TestId::new(123);
        assert_eq!(id.as_usize(), 123);
    }

    #[test]
    fn display() {
        assert_eq!(format!("{}", TestId::new(42)), "42");
    }

    #[test]
    fn from_u64_and_reverse() {
        let id: TestId = 42u64.into();
        assert_eq!(id.get(), 42);
        let v: u64 = TestId::new(42).into();
        assert_eq!(v, 42);
    }

    #[test]
    fn from_usize_and_reverse() {
        let id: TestId = 42usize.into();
        assert_eq!(id.get(), 42);
        let v: usize = TestId::new(42).into();
        assert_eq!(v, 42);
    }

    #[test]
    fn equality_and_ordering() {
        assert_eq!(TestId::new(1), TestId::new(1));
        assert_ne!(TestId::new(1), TestId::new(2));
        assert!(TestId::new(1) < TestId::new(2));
        assert!(TestId::new(2) > TestId::new(1));
        assert_eq!(TestId::new(1), 1u64);
        assert_ne!(TestId::new(1), 2u64);
        assert!(TestId::new(1) < 2u64);
        assert!(TestId::new(2) > 1u64);
    }

    #[test]
    fn saturating_arithmetic() {
        assert_eq!(TestId::new(10).saturating_add(5).get(), 15);
        assert_eq!(TestId::new(10).saturating_sub(5).get(), 5);
        assert_eq!(TestId::new(u64::MAX).saturating_add(1).get(), u64::MAX);
        assert_eq!(TestId::new(0).saturating_sub(1).get(), 0);
    }

    #[test]
    fn checked_arithmetic() {
        assert_eq!(TestId::new(10).checked_add(5), Some(TestId::new(15)));
        assert_eq!(TestId::new(10).checked_sub(5), Some(TestId::new(5)));
        assert_eq!(TestId::new(10).checked_mul(3), Some(TestId::new(30)));
        assert_eq!(TestId::new(u64::MAX).checked_add(1), None);
        assert_eq!(TestId::new(0).checked_sub(1), None);
        assert_eq!(TestId::new(u64::MAX).checked_mul(2), None);
    }

    #[test]
    fn add_sub_with_u64_and_self() {
        assert_eq!(TestId::new(1) + 2u64, TestId::new(3));
        assert_eq!(TestId::new(3) - 2u64, TestId::new(1));
        assert_eq!(TestId::new(1) + TestId::new(2), TestId::new(3));
        assert_eq!(TestId::new(3) - TestId::new(2), TestId::new(1));
    }

    #[test]
    fn add_assign_sub_assign() {
        let mut id = TestId::new(1);
        id += 2u64;
        assert_eq!(id, TestId::new(3));
        id -= TestId::new(1);
        assert_eq!(id, TestId::new(2));
    }

    #[test]
    fn serde_roundtrip() {
        let id = TestId::new(42);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "42");
        let decoded: TestId = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, id);

        let max = TestId::new(u64::MAX);
        let json = serde_json::to_string(&max).unwrap();
        let decoded: TestId = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, max);
    }
}
