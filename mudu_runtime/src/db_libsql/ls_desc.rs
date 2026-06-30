use libsql::Connection;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;

/// Get schema information for a SQL query result set
/// This function executes the query with LIMIT 0 to get only the structure without data
pub async fn desc_projection(conn: &Connection, query: &str) -> Result<Vec<DatumDesc>, MuduError> {
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
        .map_err(|e| mudu_error!(ErrorCode::Database, "prepare limit sql error", e))?;
    let column_count = stmt.column_count();

    let mut schema = Vec::with_capacity(column_count);
    let columns = stmt.columns();
    for column in columns {
        let decl_type = column
            .decl_type()
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidType, "column has no declared type"))?;
        let id = sqlite_decl_type_to_id(decl_type)?;
        let desc = DatumDesc::new(column.name().to_string(), DatType::default_for(id));

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
            return Err(mudu_error!(ErrorCode::InvalidType, "not supported type"));
        }
    };
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // libsql calls SQLite C functions that Miri does not support, so this test
    // is ignored under Miri and runs only on native builds.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_basic() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            test_sql().await.unwrap();
        })
        .unwrap();
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
                .map_err(|e| mudu_error!(ErrorCode::Database, "run sql ddl error", e))?;
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

#[cfg(test)]
#[path = "ls_desc_test.rs"]
mod ls_desc_test;
