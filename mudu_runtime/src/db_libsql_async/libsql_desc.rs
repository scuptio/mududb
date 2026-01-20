use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::error::err::MError;
use mudu::m_error;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use libsql::Statement;

/// Get schema information for a SQL query result set
/// This function executes the query with LIMIT 0 to get only the structure without data
pub async fn desc_projection(stmt: &Statement) -> Result<Vec<DatumDesc>, MError> {
    let columns = stmt.columns();
    let mut schema = Vec::with_capacity(columns.len());
    for column in columns {
        let type_str = column.decl_type()
            .map_or_else(|| { Err(m_error!(EC::NoneErr, "cannot get libsql column type")) },
                         |t| { Ok(t.to_string()) })?;
        let id = libsql_sqlite_decl_type_to_id(&type_str)?;
        let desc = DatumDesc::new(
            column.name().to_string(),
            DatType::default_for(id),
        );

        schema.push(desc);
    }

    Ok(schema)
}

fn libsql_sqlite_decl_type_to_id(name: &str) -> RS<DatTypeID> {
    let id = match name {
        "TEXT" => DatTypeID::String,
        "INT" | "INTEGER" => DatTypeID::I32,
        "BIGINT" => DatTypeID::I64,
        "REAL" => DatTypeID::F64,
        _ => {
            return Err(m_error!(EC::TypeErr, format!("do not support type {}", name)));
        }
    };
    Ok(id)
}


