//! Wallet business functions shared by the native application and MPK.
use mududb::{
    Mudu, UniCommandArgv, UniDataValue, UniOid, UniQueryArgv, UniScalarValue, UniSqlParam,
    UniSqlStmt,
};
pub type Result<T> = std::result::Result<T, String>;
fn i(value: i64) -> UniDataValue {
    UniDataValue::Scalar(UniScalarValue::I64(value))
}
fn text(value: &str) -> UniDataValue {
    UniDataValue::Scalar(UniScalarValue::String(value.into()))
}
pub async fn command(session: &UniOid, sql: &str, params: Vec<UniDataValue>) -> Result<u64> {
    let response = Mudu::command(&UniCommandArgv {
        oid: session.clone(),
        command: UniSqlStmt {
            sql_string: sql.into(),
        },
        param_list: UniSqlParam {
            params,
            param_names: None,
        },
        param_desc: None,
    })
    .await
    .map_err(|e| e.to_string())?;
    response
        .require_ok()
        .map(|r| r.affected_rows)
        .map_err(|e| e.err_msg)
}
pub async fn balance(session: &UniOid, id: i64) -> Result<i64> {
    let response = Mudu::query(&UniQueryArgv {
        oid: session.clone(),
        query: UniSqlStmt {
            sql_string: "SELECT balance FROM wallets WHERE user_id = ?".into(),
        },
        param_list: UniSqlParam {
            params: vec![i(id)],
            param_names: None,
        },
        param_desc: None,
    })
    .await
    .map_err(|e| e.to_string())?;
    let result = response.require_ok().map_err(|e| e.err_msg)?;
    match result
        .result_set
        .row_set
        .first()
        .and_then(|r| r.fields.first())
    {
        Some(UniDataValue::Scalar(UniScalarValue::I64(n))) => Ok(*n),
        Some(UniDataValue::Scalar(UniScalarValue::I32(n))) => Ok(i64::from(*n)),
        _ => Err("wallet not found".into()),
    }
}
async fn set(session: &UniOid, id: i64, value: i64) -> Result<i64> {
    if command(
        session,
        "UPDATE wallets SET balance = ? WHERE user_id = ?",
        vec![i(value), i(id)],
    )
    .await?
        != 1
    {
        return Err("wallet update failed".into());
    }
    Ok(value)
}
pub async fn create_user(session: &UniOid, id: i64, name: &str, email: &str) -> Result<i64> {
    command(
        session,
        "INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        vec![i(id), text(name), text(email), i(0), i(0)],
    )
    .await?;
    command(
        session,
        "INSERT INTO wallets (user_id, balance, updated_at) VALUES (?, ?, ?)",
        vec![i(id), i(0), i(0)],
    )
    .await?;
    Ok(id)
}
pub async fn deposit(session: &UniOid, id: i64, amount: i64) -> Result<i64> {
    if amount <= 0 {
        return Err("amount must be positive".into());
    }
    let value = balance(session, id)
        .await?
        .checked_add(amount)
        .ok_or("balance overflow")?;
    set(session, id, value).await
}
pub async fn withdraw(session: &UniOid, id: i64, amount: i64) -> Result<i64> {
    if amount <= 0 {
        return Err("amount must be positive".into());
    }
    let value = balance(session, id).await?;
    if value < amount {
        return Err("insufficient funds".into());
    }
    set(session, id, value - amount).await
}
pub async fn transfer_funds(session: &UniOid, from: i64, to: i64, amount: i64) -> Result<i64> {
    if from == to || amount <= 0 {
        return Err("invalid transfer".into());
    }
    let from_balance = balance(session, from).await?;
    let to_balance = balance(session, to).await?;
    if from_balance < amount {
        return Err("insufficient funds".into());
    }
    let next = to_balance.checked_add(amount).ok_or("balance overflow")?;
    set(session, from, from_balance - amount).await?;
    set(session, to, next).await?;
    Ok(from_balance - amount)
}
#[cfg(target_arch = "wasm32")]
mod guest;
