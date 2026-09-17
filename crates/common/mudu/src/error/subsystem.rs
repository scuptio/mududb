//! Example of a subsystem-local error enum that converts into [`MuduError`].
//!
//! This pattern lets individual crates/modules define rich, typed errors without
//! bloating the global [`ErrorCode`] protocol enum. Only the final protocol code needs
//! to be stable.

use crate::error::ErrorCode;
use crate::error::MuduError;
use std::fmt;

/// Errors originating from the catalog subsystem.
#[derive(Debug, Clone)]
pub enum CatalogError {
    TableNotFound { name: String },
    ColumnNotFound { table: String, column: String },
    CorruptedMetadata { detail: String },
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CatalogError::TableNotFound { name } => write!(f, "table not found: {}", name),
            CatalogError::ColumnNotFound { table, column } => {
                write!(f, "column not found: {}.{}", table, column)
            }
            CatalogError::CorruptedMetadata { detail } => {
                write!(f, "corrupted catalog metadata: {}", detail)
            }
        }
    }
}

impl std::error::Error for CatalogError {}

impl From<CatalogError> for MuduError {
    fn from(err: CatalogError) -> Self {
        MuduError::new_with_ec_msg(ErrorCode::EntityNotFound, err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_error_maps_to_m_error() {
        let err = CatalogError::TableNotFound {
            name: "users".to_string(),
        };
        let merr: MuduError = err.into();
        assert_eq!(merr.ec(), ErrorCode::EntityNotFound);
        assert!(merr.message().contains("users"));
    }

    #[test]
    fn all_catalog_error_variants_convert_to_m_error() {
        let cases = [
            (
                CatalogError::ColumnNotFound {
                    table: "users".to_string(),
                    column: "id".to_string(),
                },
                "users.id",
            ),
            (
                CatalogError::CorruptedMetadata {
                    detail: "missing oid".to_string(),
                },
                "missing oid",
            ),
        ];
        for (err, expected) in cases {
            let merr: MuduError = err.into();
            assert_eq!(merr.ec(), ErrorCode::EntityNotFound);
            assert!(merr.message().contains(expected));
        }
    }
}
