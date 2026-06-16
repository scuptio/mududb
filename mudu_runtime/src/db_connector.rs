use crate::db_libsql::ls_conn::{create_ls_conn, db_conn_get_libsql_connection};
use crate::db_libsql_async::libsql_async_conn::create_libsql_async_conn;
use libsql::Connection;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::database::db_conn::DBConnSync;
use mudu_contract::database::sql::DBConn;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_kernel::mudu_conn::mudu_conn_async::MuduConnAsync;
use std::str::FromStr;
use std::sync::Arc;
use strum_macros::EnumString;

pub struct DBConnector {}

#[derive(EnumString)]
enum DBType {
    LibSQL,
    LibSQLAsync,
    MuduDB,
}

impl DBConnector {
    pub async fn connect(connect_string: &str) -> RS<DBConn> {
        Self::connect_with_async_runtime(connect_string, None).await
    }

    pub async fn connect_with_async_runtime(
        connect_string: &str,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    ) -> RS<DBConn> {
        let db_str_param = parse_db_connect_string(connect_string);
        let mut passing_param = Vec::new();
        let mut opt_ddl_path = None;
        let mut opt_db_type = Some(DBType::LibSQL);
        let mut opt_db_path = None;
        let mut opt_app = None;
        for key_value in db_str_param {
            let (key, value) = parse_key_value(&key_value)?;
            match key.as_str() {
                "ddl" => {
                    opt_ddl_path = Some(value);
                }
                "db_type" => {
                    let db = DBType::from_str(&value).unwrap();
                    opt_db_type = Some(db)
                }
                "db" => {
                    opt_db_path = Some(value);
                }
                "app" => {
                    opt_app = Some(value);
                }
                _ => {
                    passing_param.push(key_value);
                }
            }
        }

        let ddl_path = opt_ddl_path.unwrap_or_else(String::default);
        let app_name = opt_app.unwrap_or(String::default());
        let db_path = match opt_db_path {
            Some(db_path) => db_path,
            None => return Err(m_error!(EC::DBInternalError, "no db path specified")),
        };
        match opt_db_type {
            Some(db_type) => match db_type {
                DBType::LibSQL => create_ls_conn(&db_path, &app_name, &ddl_path),
                DBType::LibSQLAsync => create_libsql_async_conn(&db_path, &app_name).await,
                DBType::MuduDB => create_mudu_conn(async_runtime).await,
            },
            None => Err(m_error!(EC::ParseErr, "not a valid DB type")),
        }
    }

    pub fn get_libsql_conn(db_conn: &dyn DBConnSync) -> Option<Connection> {
        db_conn_get_libsql_connection(db_conn)
    }
}

async fn create_mudu_conn(async_runtime: Option<Arc<dyn AsyncIoProvider>>) -> RS<DBConn> {
    Ok(DBConn::Async(Arc::new(MuduConnAsync::new_with_runtime(
        async_runtime,
    )?)))
}

fn parse_key_value(s: &str) -> RS<(String, String)> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(m_error!(
            EC::ParseErr,
            format!("Invalid pull-push pair: '{}'", s)
        ));
    }

    let key = parts[0].to_string();
    let value = parts[1].to_string();

    let value = if value.starts_with('\'') && value.ends_with('\'') {
        value[1..value.len() - 1].to_string()
    } else {
        value
    };

    Ok((key, value))
}

fn parse_db_connect_string(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;

    for c in input.chars() {
        match c {
            '\'' => {
                in_quote = !in_quote;
                current.push(c);
            }
            _ if c.is_whitespace() && !in_quote => {
                if !current.is_empty() {
                    result.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_db_connect_string() {
        assert_eq!(
            parse_db_connect_string("host=localhost port=5432 user=postgres"),
            vec!["host=localhost", "port=5432", "user=postgres"]
        );

        assert_eq!(
            parse_db_connect_string("host='localhost server' port=5432 password='my password'"),
            vec![
                "host='localhost server'",
                "port=5432",
                "password='my password'"
            ]
        );

        assert_eq!(
            parse_db_connect_string("  host=localhost  port=5432  "),
            vec!["host=localhost", "port=5432"]
        );

        assert_eq!(
            parse_db_connect_string("'host=localhost port=5432'"),
            vec!["'host=localhost port=5432'"]
        );
    }

    #[test]
    fn test_parse_key_value() {
        assert_eq!(
            parse_key_value("host=localhost"),
            Ok(("host".to_string(), "localhost".to_string()))
        );

        assert_eq!(
            parse_key_value("password='my password'"),
            Ok(("password".to_string(), "my password".to_string()))
        );

        assert!(parse_key_value("invalid").is_err());
    }
}
