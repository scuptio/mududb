use crate::db_libsql::ls_conn::create_ls_conn;
use crate::db_postgres::pg_interactive_conn::create_pg_interactive_conn;
use mudu::common::result::RS;
use mudu::database::db_conn::DBConn;
use mudu::error::ec::EC;
use mudu::m_error;
use std::str::FromStr;
use std::sync::Arc;
use strum_macros::EnumString;

pub struct DBConnector {}

#[derive(EnumString)]
enum DBType {
    Postgres,
    LibSQL,
}

impl DBConnector {
    pub fn connect(
        connect_string: &str,
    ) -> RS<Arc<dyn DBConn>> {
        let db_str_param = parse_db_connect_string(connect_string);
        let mut passing_param = Vec::new();
        let mut opt_ddl_path = None;
        let mut opt_db_type = Some(DBType::Postgres);
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

        let ddl_path = match opt_ddl_path {
            Some(ddl_path) => { ddl_path }
            None => {
                String::default()
            }
        };
        let app_name = opt_app.unwrap_or(String::default());
        let params = merge_to_string(passing_param);
        match opt_db_type {
            Some(db_type) => {
                match db_type {
                    DBType::Postgres => {
                        create_pg_interactive_conn(&params, &ddl_path)
                    }
                    DBType::LibSQL => {
                        create_ls_conn(&opt_db_path.unwrap(), &app_name, &ddl_path)
                    }
                }
            }
            None => {
                Err(m_error!(EC::ParseErr, "not a valid DB type"))
            }
        }
    }
}

fn parse_key_value(s: &str) -> RS<(String, String)> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(m_error!(EC::ParseErr, format!("Invalid key-value pair: '{}'", s)));
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

fn merge_to_string(vec: Vec<String>) -> String {
    let n = vec.len();
    let mut ret = String::new();
    for (i, s) in vec.iter().enumerate() {
        ret.push_str(s);
        if i != n {
            ret.push_str(" ");
        }
    }
    ret
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
            vec!["host='localhost server'", "port=5432", "password='my password'"]
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

        assert!(
            parse_key_value("invalid").is_err()
        );
    }
}