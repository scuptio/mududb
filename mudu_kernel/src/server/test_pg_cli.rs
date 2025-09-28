#[cfg(test)]
pub mod test {
    use postgres::{Client, NoTls};

    use std::thread::sleep;
    use std::time::Duration;
    use tracing::{error, info};

    enum TestResult {
        Query(Result<Vec<Vec<String>>, String>),
        Command(Result<u64, String>),
    }
    pub struct TestSQL {
        sql: String,
        result: TestResult,
    }

    impl TestSQL {
        pub fn from_query(sql: String, result: Result<Vec<Vec<String>>, String>) -> Self {
            Self {
                sql,
                result: TestResult::Query(result),
            }
        }

        pub fn from_command(sql: String, result: Result<u64, String>) -> Self {
            Self {
                sql,
                result: TestResult::Command(result),
            }
        }
    }

    pub fn run_pg_client(pg_host: String, user: String, password: String, vec_sql: Vec<TestSQL>) {
        let mut client = loop {
            let connect_str = format!("host={} user={} password={}", pg_host, user, password);
            let r = Client::connect(&connect_str, NoTls);
            match r {
                Ok(c) => break c,
                Err(e) => {
                    info!("{:?}, {:?}, {:?}", e, e.code(), e.as_db_error());
                    sleep(Duration::from_millis(1000));
                }
            }
        };
        for stmt in vec_sql {
            let sql = stmt.sql;
            match stmt.result {
                TestResult::Command(r_expected) => {
                    let r_executed = client.execute(&sql, &[]);
                    match (&r_executed, &r_expected) {
                        (Ok(rows_executed), Ok(rows_expected)) => {
                            assert_eq!(rows_executed, rows_expected);
                            info!("{} rows affected", rows_executed);
                        }
                        (Err(e), Err(_e)) => {
                            error!("{:?}, {:?}, {:?}", e, e.code(), e.as_db_error());
                        }
                        _ => {
                            error!("mismatch result {:?}, {:?}", r_executed, r_expected);
                            panic!("error mismatch result");
                        }
                    };
                }
                TestResult::Query(r_expected) => {
                    let r_executed = client.query(&sql, &[]);
                    match (r_executed, r_expected) {
                        (Ok(rows_executed), Ok(rows_expected)) => {
                            let rows: Vec<_> = rows_executed
                                .iter()
                                .map(|row| {
                                    let mut vec = vec![];
                                    for i in 0..row.len() {
                                        let s: String = row.get(i);
                                        vec.push(s);
                                    }
                                    vec
                                })
                                .collect();
                            println!("{:?} result rows", rows);
                        }
                        (Err(e), Err(_e)) => {
                            error!("{:?}, {:?}, {:?}", e, e.code(), e.as_db_error());
                        }
                        _ => {
                            panic!("error mismatch result");
                        }
                    }
                }
            }
        }
    }
}
