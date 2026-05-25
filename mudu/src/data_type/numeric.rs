use bigdecimal::rounding::RoundingMode;
use bigdecimal::{BigDecimal, ParseBigDecimalError};
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// Stable Mudu wrapper around `bigdecimal::BigDecimal`.
///
/// Database-facing crates should depend on this wrapper rather than on the
/// third-party type directly so numeric semantics can evolve behind a single
/// abstraction boundary.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Numeric {
    inner: BigDecimal,
}

impl Numeric {
    pub fn new(inner: BigDecimal) -> Self {
        Self { inner }
    }

    pub fn zero() -> Self {
        Self::from(0_i32)
    }

    pub fn parse(value: &str) -> Result<Self, ParseBigDecimalError> {
        BigDecimal::from_str(value).map(Self::new)
    }

    pub fn from_bigdecimal(inner: BigDecimal) -> Self {
        Self::new(inner)
    }

    pub fn as_bigdecimal(&self) -> &BigDecimal {
        &self.inner
    }

    pub fn into_bigdecimal(self) -> BigDecimal {
        self.inner
    }

    pub fn with_scale(&self, new_scale: i64) -> Self {
        Self::new(self.inner.with_scale(new_scale))
    }

    pub fn with_scale_round(&self, new_scale: i64, mode: RoundingMode) -> Self {
        Self::new(self.inner.with_scale_round(new_scale, mode))
    }

    pub fn round_half_even(&self, new_scale: i64) -> Self {
        self.with_scale_round(new_scale, RoundingMode::HalfEven)
    }

    pub fn to_plain_string(&self) -> String {
        self.inner.to_plain_string()
    }
}

impl Default for Numeric {
    fn default() -> Self {
        Self::zero()
    }
}

impl From<BigDecimal> for Numeric {
    fn from(value: BigDecimal) -> Self {
        Self::new(value)
    }
}

impl From<Numeric> for BigDecimal {
    fn from(value: Numeric) -> Self {
        value.into_bigdecimal()
    }
}

macro_rules! impl_numeric_from_int {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl From<$ty> for Numeric {
                fn from(value: $ty) -> Self {
                    Self::new(BigDecimal::from(value))
                }
            }
        )+
    };
}

impl_numeric_from_int!(i8, i16, i32, i64, i128, u8, u16, u32, u64, u128);

impl FromStr for Numeric {
    type Err = ParseBigDecimalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Display for Numeric {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_plain_string().as_str())
    }
}

impl Serialize for Numeric {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_plain_string().as_str())
    }
}

impl<'de> Deserialize<'de> for Numeric {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value.as_str()).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::Numeric;
    use bigdecimal::rounding::RoundingMode;

    #[test]
    fn serializes_as_plain_string() {
        let value = Numeric::parse("12.3400").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, "\"12.3400\"");
    }

    #[test]
    fn deserializes_from_string() {
        let value: Numeric = serde_json::from_str("\"-0.0100\"").unwrap();
        assert_eq!(value.to_string(), "-0.0100");
    }

    #[test]
    fn rounds_with_explicit_mode() {
        let value = Numeric::parse("1.235").unwrap();
        let rounded = value.with_scale_round(2, RoundingMode::HalfEven);
        assert_eq!(rounded.to_string(), "1.24");
    }
}
