use crate::postgres::db_conn_pg::DBConnPG;
use mudu::common::error::ER;
use mudu::common::result::RS;
use mudu::database::db_conn::DBConn;
use std::sync::Arc;

pub struct DBConnector {
    
}

enum DBType {
    Postgres,
}

impl DBConnector {
    pub fn connect(
        connect_string:&str,
    ) -> RS<Arc<dyn DBConn>> {
        let db_str_param = parse_db_connect_string(connect_string);
        let mut passing_param = Vec::new();
        let mut opt_ddl_path = None;
        let mut opt_db_type = Some(DBType::Postgres);
        for key_value in db_str_param {
            let (key, value) = parse_key_value(&key_value)?;
            match key.as_str() {
                "ddl" => {
                    opt_ddl_path = Some(value);
                }
                "db_type" => {
                    opt_db_type = Some(DBType::Postgres)
                }
                _ => {
                    passing_param.push(key_value);
                }
            } 
        }
        let params = merge_to_string(passing_param);
        match opt_db_type {
            Some(DBType::Postgres) => {
                let conn = DBConnPG::new(&params, &opt_ddl_path.unwrap())?;
                Ok(Arc::new(conn))
            }
            None => {
                panic!("unknown DB type")
            }
        }
    }
}

fn parse_key_value(s:&str) -> RS<(String, String)> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(ER::ParseError(format!("Invalid key-value pair: '{}'", s)));
    }

    let key = parts[0].to_string();
    let value = parts[1].to_string();

   
    let value = if value.starts_with('\'') && value.ends_with('\'') {
        value[1..value.len()-1].to_string()
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

fn merge_to_string(vec:Vec<String>) -> String {
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