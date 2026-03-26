use mudu_api_rust::{
    MockSqliteMuduSysCall, Mudu, UniCommandArgv, UniDatValue, UniOid, UniPrimitiveValue,
    UniQueryArgv, UniSqlParam, UniSqlStmt,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = std::env::current_dir()?
        .join("mudu_api")
        .join("rust")
        .join("demo")
        .join("demo.db");
    MockSqliteMuduSysCall::set_database_path(&db_path);

    run_command(
        "create table if not exists demo_users (\
         id integer primary key autoincrement, \
         name text not null, \
         score integer not null\
         )",
    )
    .await?;

    run_command("delete from demo_users").await?;
    insert_user("alice", 10).await?;
    insert_user("bob", 20).await?;

    let query = UniQueryArgv {
        oid: UniOid { h: 0, l: 0 },
        query: UniSqlStmt {
            sql_string: "select id, name, score from demo_users where score >= ? order by id"
                .into(),
        },
        param_list: UniSqlParam {
            params: vec![UniDatValue::Primitive(UniPrimitiveValue::I32(10))],
        },
    };

    let response = Mudu::query(&query).await?;
    let result = response
        .require_ok()
        .map_err(|error| format!("query failed: {} {}", error.err_code, error.err_msg))?;

    println!("db: {}", db_path.display());
    println!("rows: {}", result.result_set.row_set.len());

    for row in result.result_set.row_set {
        let id = match &row.fields[0] {
            UniDatValue::Primitive(UniPrimitiveValue::I64(value)) => *value,
            other => return Err(format!("unexpected id field: {other:?}").into()),
        };
        let name = match &row.fields[1] {
            UniDatValue::Primitive(UniPrimitiveValue::String(value)) => value.clone(),
            other => return Err(format!("unexpected name field: {other:?}").into()),
        };
        let score = match &row.fields[2] {
            UniDatValue::Primitive(UniPrimitiveValue::I64(value)) => *value,
            other => return Err(format!("unexpected score field: {other:?}").into()),
        };

        println!("{id}: {name} -> {score}");
    }

    Ok(())
}

async fn insert_user(name: &str, score: i32) -> Result<(), Box<dyn std::error::Error>> {
    let argv = UniCommandArgv {
        oid: UniOid { h: 0, l: 0 },
        command: UniSqlStmt {
            sql_string: "insert into demo_users(name, score) values(?, ?)".into(),
        },
        param_list: UniSqlParam {
            params: vec![
                UniDatValue::Primitive(UniPrimitiveValue::String(name.to_string())),
                UniDatValue::Primitive(UniPrimitiveValue::I32(score)),
            ],
        },
    };

    let response = Mudu::command(&argv).await?;
    response
        .require_ok()
        .map(|_| ())
        .map_err(|error| format!("insert failed: {} {}", error.err_code, error.err_msg).into())
}

async fn run_command(sql: &str) -> Result<(), Box<dyn std::error::Error>> {
    let argv = UniCommandArgv {
        oid: UniOid { h: 0, l: 0 },
        command: UniSqlStmt {
            sql_string: sql.to_string(),
        },
        param_list: UniSqlParam { params: Vec::new() },
    };

    let response = Mudu::command(&argv).await?;
    response
        .require_ok()
        .map(|_| ())
        .map_err(|error| format!("command failed: {} {}", error.err_code, error.err_msg).into())
}
