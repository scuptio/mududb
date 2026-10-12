use mududb::{Mudu, UniOid};
use wallet_rs::*;
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    mududb::adapter::use_adapter(std::env::var("MUDU_ADAPTER_LIBRARY")?)?;
    let session = Mudu::open_session(&UniOid::default())
        .await?
        .require_ok()
        .map_err(|e| e.err_msg)?;
    let result = run(&session).await;
    let closed = Mudu::close_session(&session)
        .await?
        .require_ok()
        .map_err(|e| e.err_msg);
    result?;
    closed?;
    Ok(())
}
async fn run(session: &UniOid) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |n: usize| args.get(n).ok_or("missing argument");
    match arg(0)?.as_str() {
        "init" => {
            for sql in include_str!("../../mpk/sql/ddl.sql").split(';') {
                if !sql.trim().is_empty() {
                    command(session, sql, vec![]).await?;
                }
            }
        }
        "scenario" => {
            create_user(session, 1001, "Alice", "alice@example.com").await?;
            create_user(session, 1002, "Bob", "bob@example.com").await?;
            deposit(session, 1001, 1000).await?;
            withdraw(session, 1001, 100).await?;
            transfer_funds(session, 1001, 1002, 250).await?;
            if balance(session, 1001).await? != 650 || balance(session, 1002).await? != 250 {
                return Err("balance mismatch".into());
            }
            if withdraw(session, 1001, 10000).await.err().as_deref() != Some("insufficient funds") {
                return Err("insufficient funds accepted".into());
            }
            println!("{{\"alice\":650,\"bob\":250}}");
        }
        "balance" => println!("{}", balance(session, arg(1)?.parse()?).await?),
        "create" => println!(
            "{}",
            create_user(session, arg(1)?.parse()?, arg(2)?, arg(3)?).await?
        ),
        "deposit" => println!(
            "{}",
            deposit(session, arg(1)?.parse()?, arg(2)?.parse()?).await?
        ),
        "withdraw" => println!(
            "{}",
            withdraw(session, arg(1)?.parse()?, arg(2)?.parse()?).await?
        ),
        "transfer" => println!(
            "{}",
            transfer_funds(
                session,
                arg(1)?.parse()?,
                arg(2)?.parse()?,
                arg(3)?.parse()?
            )
            .await?
        ),
        _ => {
            return Err(
                "usage: init | scenario | create | deposit | withdraw | transfer | balance".into(),
            );
        }
    };
    Ok(())
}
