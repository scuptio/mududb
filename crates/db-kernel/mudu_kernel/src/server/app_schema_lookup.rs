//! Application schema lookup for pre-flight parameter type checking.
//!
//! `mudu_kernel` cannot depend on `mudu_runtime` (where the per-app
//! `SchemaMgr` lives), so the runtime registers a lookup provider here and
//! the client-protocol handlers query it. When no provider is registered,
//! or the application has no registered schema, the type check is skipped
//! (best-effort semantics).

use lazy_static::lazy_static;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::table::table_def::TableDef;
use mudu_sys::sync::SRwLock;
use mudu_type::data_value::DataValue;
use sql_parser::check::param_type::{
    param_target_columns, param_type_compatible, param_type_tag_of_value, uni_data_type_name,
};
use std::sync::Arc;

/// Provides table schemas for an application by name.
pub trait AppSchemaLookup: Send + Sync {
    /// Return the table definitions known for the application, or `None`
    /// when the application has no registered schema.
    fn schema(&self, app_name: &str) -> Option<Vec<TableDef>>;
}

lazy_static! {
    static ref LOOKUP: SRwLock<Option<Arc<dyn AppSchemaLookup>>> = SRwLock::new(None);
}

/// Register the application schema lookup provider (called by the runtime).
pub fn register_app_schema_lookup(lookup: Arc<dyn AppSchemaLookup>) -> RS<()> {
    let mut guard = LOOKUP.write()?;
    *guard = Some(lookup);
    Ok(())
}

/// Return the schema of `app_name` when a provider is registered and the
/// application is known to it.
pub fn app_schema(app_name: &str) -> Option<Vec<TableDef>> {
    let guard = LOOKUP.read().ok()?;
    guard.as_ref().and_then(|lookup| lookup.schema(app_name))
}

/// Remove the registered provider (test cleanup only).
#[cfg(test)]
pub(crate) fn unregister_app_schema_lookup_for_test() {
    if let Ok(mut guard) = LOOKUP.write() {
        *guard = None;
    }
}

/// Pre-flight parameter type check for the client protocol: map every `?`
/// placeholder to its target column (using the shared `sql_parser` API)
/// and compare the parameter value's type tag with the column type using
/// the shared compatibility table.
///
/// Best-effort: skipped entirely when the application has no registered
/// schema, and per-placeholder when the placeholder has no statically
/// known column (beyond-subset SQL, `? = ?`, ...). A clear mismatch is an
/// `InvalidType` error naming the parameter index, the column, and both
/// types.
pub fn check_param_types(app_name: &str, sql: &str, params: &[DataValue]) -> RS<()> {
    let Some(schema) = app_schema(app_name) else {
        return Ok(());
    };
    let columns = param_target_columns(sql, &schema)?;
    for (index, (value, column)) in params.iter().zip(columns.iter()).enumerate() {
        let Some(column) = column else {
            continue;
        };
        let tag = param_type_tag_of_value(value);
        if !param_type_compatible(tag, column.data_type()) {
            return Err(mudu_error!(
                ErrorCode::InvalidType,
                format!(
                    "parameter {index} of type {} is not compatible with column '{}' of type {}",
                    tag.label(),
                    column.column_name(),
                    uni_data_type_name(column.data_type())
                )
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use mudu_binding::table::column_def::ColumnDef;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_scalar::UniScalar;
    use mudu_sys::sync::SMutex;

    fn test_lock() -> &'static SMutex<()> {
        static LOCK: SMutex<()> = SMutex::new(());
        &LOCK
    }

    struct TestLookup;

    impl AppSchemaLookup for TestLookup {
        fn schema(&self, app_name: &str) -> Option<Vec<TableDef>> {
            if app_name != "demo" {
                return None;
            }
            Some(vec![TableDef::new(
                "wallets".to_string(),
                vec![
                    ColumnDef::new(
                        "user_id".to_string(),
                        UniDataType::from_scalar(UniScalar::I32),
                        None,
                        true,
                        true,
                    ),
                    ColumnDef::new(
                        "balance".to_string(),
                        UniDataType::from_scalar(UniScalar::I32),
                        None,
                        false,
                        false,
                    ),
                    ColumnDef::new(
                        "note".to_string(),
                        UniDataType::from_scalar(UniScalar::String),
                        None,
                        false,
                        false,
                    ),
                ],
            )])
        }
    }

    fn with_lookup<T>(f: impl FnOnce() -> T) -> T {
        let _guard = test_lock().lock().unwrap();
        register_app_schema_lookup(Arc::new(TestLookup)).unwrap();
        let result = f();
        unregister_app_schema_lookup_for_test();
        result
    }

    const INSERT: &str = "INSERT INTO wallets (user_id, balance, note) VALUES (?, ?, ?)";

    #[test]
    fn matching_params_pass() {
        with_lookup(|| {
            let params = vec![
                DataValue::from_i32(1),
                DataValue::from_i32(100),
                DataValue::from_string("x".to_string()),
            ];
            check_param_types("demo", INSERT, &params).unwrap();
            // Integer widening: an i64 value fits an I32 column.
            let params = vec![
                DataValue::from_i32(1),
                DataValue::from_i64(100),
                DataValue::from_string("x".to_string()),
            ];
            check_param_types("demo", INSERT, &params).unwrap();
        });
    }

    #[test]
    fn mismatched_param_reports_column_and_types() {
        with_lookup(|| {
            let params = vec![
                DataValue::from_i32(1),
                DataValue::from_string("abc".to_string()),
                DataValue::from_string("x".to_string()),
            ];
            let err = check_param_types("demo", INSERT, &params).unwrap_err();
            assert_eq!(err.ec(), ErrorCode::InvalidType);
            let message = err.message();
            assert!(message.contains("parameter 1"), "{message}");
            assert!(message.contains("'balance'"), "{message}");
            assert!(message.contains("I32"), "{message}");
            assert!(message.contains("text"), "{message}");
        });
    }

    #[test]
    fn null_param_fits_any_column() {
        with_lookup(|| {
            let params = vec![
                DataValue::from_i32(1),
                DataValue::null(),
                DataValue::from_string("x".to_string()),
            ];
            check_param_types("demo", INSERT, &params).unwrap();
        });
    }

    #[test]
    fn unknown_app_and_unknown_table_skip_the_check() {
        with_lookup(|| {
            // No registered schema for the app: skipped, even with a
            // mismatch that would otherwise fail.
            let params = vec![DataValue::from_string("abc".to_string())];
            check_param_types("nope", "INSERT INTO wallets (user_id) VALUES (?)", &params).unwrap();
            // Unknown table: the placeholder has no column, skipped.
            let params = vec![DataValue::from_string("abc".to_string())];
            check_param_types("demo", "INSERT INTO nope (x) VALUES (?)", &params).unwrap();
            // Beyond-subset SQL (JOIN): unmappable, skipped.
            let params = vec![DataValue::from_string("abc".to_string())];
            check_param_types(
                "demo",
                "SELECT * FROM wallets w JOIN wallets u ON w.user_id = u.user_id WHERE u.user_id = ?",
                &params,
            )
            .unwrap();
        });
    }

    #[test]
    fn where_clause_mismatch_is_caught() {
        with_lookup(|| {
            let params = vec![DataValue::from_string("abc".to_string())];
            let err = check_param_types(
                "demo",
                "SELECT balance FROM wallets WHERE user_id = ?",
                &params,
            )
            .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::InvalidType);
            assert!(err.message().contains("'user_id'"), "{}", err.message());
        });
    }
}
