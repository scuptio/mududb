#[cfg(test)]
mod test {
    use crate::backend::test_backend::tests::test_backend;
    use crate::backend::test_pg_cli::test::{TestSQL, run_pg_client};
    use mudu_utils::log::log_setup;
    use std::thread::JoinHandle;
    use tracing::info;

    //#[test]
    #[allow(unused)]
    fn run_test_sql() {
        log_setup("info");
        let server = mudu_serve();
        let cli = pg_client();
        server.join().unwrap();
        cli.join().unwrap();
        info!("run_test_sql test success");
    }

    fn mudu_serve() -> JoinHandle<()> {
        std::thread::spawn(|| {
            test_backend().unwrap();
        })
    }

    fn pg_client() -> JoinHandle<()> {
        let thd = std::thread::spawn(move || _run_pg_client());
        thd
    }

    fn _run_pg_client() {
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
            "app1".to_string(),
            "root".to_string(),
            "root".to_string(),
            test_sql,
        );
    }
}
