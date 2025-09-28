#[cfg(test)]
mod test {
    use crate::server::mudu_server::{MuduServer, MuduStop};
    use project_root::get_project_root;

    use crate::server::test_pg_cli::test::{run_pg_client, TestSQL};
    use mudu_utils::log::log_setup;
    use std::thread::JoinHandle;
    use tracing::info;

    #[test]
    fn run_test_sql() {
        log_setup("debug");
        let (server, stop) = mudu_serve().unwrap();
        let cli = pg_client(stop);
        server.join().unwrap();
        cli.join().unwrap();
        info!("run_test_sql test success");
    }

    fn mudu_serve() -> Result<(MuduServer, MuduStop), Box<dyn std::error::Error>> {
        log_setup("info");
        let mut cfg_path = get_project_root().unwrap();
        cfg_path.push("cfg/test.toml");
        let cfg_path = cfg_path.to_str().unwrap().to_string();
        let r_server = MuduServer::start(Some(cfg_path));
        let server = r_server.map_err(|e| Box::new(e))?;
        Ok(server)
    }

    fn pg_client(stop: MuduStop) -> JoinHandle<()> {
        let thd = std::thread::spawn(move || _run_pg_client(stop));
        thd
    }

    fn _run_pg_client(stop: MuduStop) {
        let test_sql = vec![
            TestSQL::from_command(
                r#"
                CREATE TABLE T1(
                       C1      INT,
                       C2      INT,
                       C3      CHAR (20),
                       C4      INT,
                       C5      VARCHAR (25),
                       C6      INT,
                       PRIMARY KEY (C1, C2)
                );
                "#
                    .to_string(),
                Ok(0),
            ),
            TestSQL::from_command(
                r#"
                INSERT INTO T1 (C1,C2,C3,C4,C5,C6)
                    VALUES (1,1,'aaabbbccc1',
                        1,'1323456',1);
                "#
                    .to_string(),
                Ok(1),
            ),
            TestSQL::from_command(
                r#"
                INSERT INTO T1 (C3,C4,C5,C6, C2, C1)
                    VALUES ('aaabbbccc2',
                        2,'13234562',2, 2, 2);
                "#
                    .to_string(),
                Ok(1),
            ),
            TestSQL::from_command(
                r#"
                SELECT C4, C1, C2, C3, C2, C5 FROM T1 WHERE C1 = 1 AND C2 = 1;
                "#
                    .to_string(),
                Ok(1),
            ),
        ];
        run_pg_client(
            "localhost".to_string(),
            "postgres".to_string(),
            "root".to_string(),
            test_sql,
        );
        stop.stop();
    }
}
