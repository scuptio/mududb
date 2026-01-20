use libsql::Connection;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::error::err::MError;
use mudu::m_error;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;

/// Get schema information for a SQL query result set
/// This function executes the query with LIMIT 0 to get only the structure without data
pub async fn desc_projection(conn: &Connection, query: &str) -> Result<Vec<DatumDesc>, MError> {
    // Use LIMIT 0 to get only structure without data

    let _query = query
        .to_lowercase()
        .trim()
        .trim_matches(';')
        .trim()
        .to_string();
    let limited_query = format!("SELECT * FROM ({}) LIMIT 0", _query);

    let stmt = conn
        .prepare(&limited_query)
        .await
        .map_err(|e| m_error!(EC::DBInternalError, "prepare limit sql error", e))?;
    let column_count = stmt.column_count();

    let mut schema = Vec::with_capacity(column_count);
    let columns = stmt.columns();
    for i in 0..column_count {
        let column = &columns[i];
        let id = sqlite_decl_type_to_id(column.decl_type().unwrap())?;
        let desc = DatumDesc::new(
            column.name().to_string(),
            DatType::default_for(id),
        );

        schema.push(desc);
    }

    Ok(schema)
}

fn sqlite_decl_type_to_id(name: &str) -> RS<DatTypeID> {
    let id = match name {
        "TEXT" => DatTypeID::String,
        "INT" | "INTEGER" => DatTypeID::I32,
        "BIGINT" => DatTypeID::I64,
        "REAL" => DatTypeID::F64,
        _ => {
            return Err(m_error!(EC::TypeErr, format!("not supported type")));
        }
    };
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic() -> Result<(), Box<dyn std::error::Error>> {
        test_sql().await?;
        Ok(())
    }
    async fn test_sql() -> RS<()> {
        let ddl_sql = [
            r#"-- Users table
CREATE TABLE users (
    user_id TEXT PRIMARY KEY,
    phone TEXT
);"#,
            r#"-- Votes table
CREATE TABLE votes (
    vote_id TEXT PRIMARY KEY,
    creator_id TEXT,
    topic TEXT NOT NULL,
    vote_type TEXT /*CHECK(vote_type IN ('single', 'multiple')) */,
    max_choices INTEGER,
    end_time INTEGER NOT NULL,
    visibility_rule TEXT /*CHECK(visibility_rule IN ('always', 'after_end'))*/
);"#,
            r#"-- Options table
CREATE TABLE options (
    option_id TEXT PRIMARY KEY,
    vote_id TEXT,
    option_text TEXT NOT NULL
);"#,
            r#"-- Vote actions table
CREATE TABLE vote_actions (
    action_id TEXT PRIMARY KEY,
    user_id TEXT,
    vote_id TEXT,
    action_time INTEGER NOT NULL,
    is_withdrawn INTEGER
);"#,
            r#"-- Vote choices table
CREATE TABLE vote_choices (
    choice_id TEXT PRIMARY KEY,
    action_id TEXT,
    option_id TEXT
)"#,
        ];
        let db = libsql::Builder::new_local(":memory:")
            .build()
            .await
            .unwrap();
        let conn = db.connect().unwrap();
        for sql in ddl_sql.iter() {
            conn.execute(sql, ())
                .await
                .map_err(|e| m_error!(EC::DBInternalError, "run sql ddl error", e))?;
        }

        let query = ["SELECT va.*, v.topic
             FROM vote_actions va
             JOIN votes v ON va.vote_id = v.vote_id
             WHERE user_id = 1"];
        for q in query.iter() {
            let desc = desc_projection(&conn, q).await.unwrap();
            println!("{:?}", desc);
        }

        Ok(())
    }
}
