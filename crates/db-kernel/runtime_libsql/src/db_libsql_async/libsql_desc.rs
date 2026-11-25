use libsql::Statement;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_type::data_type::DataType;
use mudu_type::type_family::TypeFamily;

/// Get schema information for a SQL query result set
/// This function executes the query with LIMIT 0 to get only the structure without data
pub(crate) async fn desc_projection(stmt: &Statement) -> Result<Vec<DatumDesc>, MuduError> {
    let columns = stmt.columns();
    let mut schema = Vec::with_capacity(columns.len());
    for column in columns {
        let type_str = column.decl_type().map_or_else(
            || {
                Err(mudu_error!(
                    ErrorCode::InvalidState,
                    "cannot get libsql column type"
                ))
            },
            |t| Ok(t.to_string()),
        )?;
        let id = libsql_sqlite_decl_type_to_id(&type_str)?;
        let desc = DatumDesc::new(column.name().to_string(), DataType::default_for(id));

        schema.push(desc);
    }

    Ok(schema)
}

fn libsql_sqlite_decl_type_to_id(name: &str) -> RS<TypeFamily> {
    let id = match name {
        "TEXT" => TypeFamily::String,
        "INT" | "INTEGER" => TypeFamily::I32,
        "BIGINT" => TypeFamily::I64,
        "REAL" => TypeFamily::F64,
        _ => {
            return Err(mudu_error!(
                ErrorCode::InvalidType,
                format!("do not support type {}", name)
            ));
        }
    };
    Ok(id)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn decl_type_to_id_mapping() {
        assert_eq!(
            libsql_sqlite_decl_type_to_id("TEXT").unwrap(),
            TypeFamily::String
        );
        assert_eq!(
            libsql_sqlite_decl_type_to_id("INT").unwrap(),
            TypeFamily::I32
        );
        assert_eq!(
            libsql_sqlite_decl_type_to_id("INTEGER").unwrap(),
            TypeFamily::I32
        );
        assert_eq!(
            libsql_sqlite_decl_type_to_id("BIGINT").unwrap(),
            TypeFamily::I64
        );
        assert_eq!(
            libsql_sqlite_decl_type_to_id("REAL").unwrap(),
            TypeFamily::F64
        );
        assert!(libsql_sqlite_decl_type_to_id("BLOB").is_err());
    }

    #[test]
    fn desc_projection_reads_columns_and_rejects_expression_columns() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let db = libsql::Builder::new_local(":memory:")
                .build()
                .await
                .unwrap();
            let conn = db.connect().unwrap();
            conn.execute("CREATE TABLE t (id INTEGER, name TEXT)", ())
                .await
                .unwrap();
            let stmt = conn.prepare("SELECT id, name FROM t").await.unwrap();
            let desc = desc_projection(&stmt).await.unwrap();
            assert_eq!(desc.len(), 2);

            // Expression columns carry no declared type.
            let expr_stmt = conn.prepare("SELECT 1 + 1").await.unwrap();
            assert!(desc_projection(&expr_stmt).await.is_err());
        });
    }
}
