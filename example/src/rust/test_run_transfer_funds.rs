#[cfg(test)]
pub mod test {
    use crate::rust::transfer_funds::transfer_funds;
    use mudu::common::result::RS;
    use mudu::database::sql;
    use mudu::error::ec::EC;
    use mudu::m_error;
    use mudu_runtime::db_connector::DBConnector;
    use postgresql_commands::psql::PsqlBuilder;
    use postgresql_commands::{CommandBuilder, CommandExecutor};
    use std::path::PathBuf;

    //#[test]
    #[allow(dead_code)]
    fn test() {
        let r = test_transfer(
            &"localhost".to_string(),
            &"postgres".to_string(),
            &"postgres".to_string(),
            &"postgres".to_string(),
        );
        match r {
            Ok(()) => {}
            Err(e) => {
                panic!("{}", e);
            }
        }
    }
    fn test_transfer(host: &String, user: &String, db_name: &String, password: &String) -> RS<()> {
        let ddl_sql_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/sql/ddl.sql")
            .to_str()
            .unwrap()
            .to_string();
        let init_sql_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/sql/init.sql")
            .to_str()
            .unwrap()
            .to_string();
        run_sql(host, user, db_name, password, &ddl_sql_path)?;
        run_sql(host, user, db_name, password, &init_sql_path)?;
        let conn = DBConnector::connect(&format!(
            "host={} user={} dbname={} ddl={}",
            host, user, db_name, ddl_sql_path
        ))?;
        let context = sql::context(conn)?;
        transfer_funds(context.xid(), 1, 2, 200)?;
        Ok(())
    }

    fn run_sql(
        host: &String,
        user: &String,
        db_name: &String,
        password: &String,
        path: &String,
    ) -> RS<()> {
        let mut psql = PsqlBuilder::new()
            .file(path)
            .host(host)
            .port(5432)
            .username(user)
            .dbname(db_name)
            .pg_password(password)
            .build();
        let r = psql.execute();
        match r {
            Ok(s) => {
                println!("{} {}", s.0, s.1)
            }
            Err(e) => {
                return Err(m_error!(EC::MuduError, "", e));
            }
        }
        Ok(())
    }
}
