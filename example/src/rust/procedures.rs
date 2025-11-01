use crate::rust::wallets::object::Wallets;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::database::attr_value::AttrValue;
use mudu::error::ec::EC::MuduError;
use mudu::tuple::datum::DatumDyn;
use mudu::{m_error, sql_params, sql_stmt};
use mudu_macro::mudu_proc;
use std::time::{SystemTime, UNIX_EPOCH};
use sys_interface::api::{mudu_command, mudu_query};
use uuid::Uuid;

fn current_timestamp() -> i64 {
    let now = SystemTime::now();
    let duration_since_epoch = now
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime before UNIX EPOCH!");

    let seconds = duration_since_epoch.as_secs();
    seconds as _
}

#[mudu_proc]
pub fn transfer_funds(xid: XID, from_user_id: i32, to_user_id: i32, amount: i32) -> RS<()> {
    // Check amount > 0
    if amount <= 0 {
        return Err(m_error!(
            MuduError,
            "The transfer amount must be greater than 0"
        ));
    }

    // Cannot transfer money to oneself
    if from_user_id == to_user_id {
        return Err(m_error!(MuduError, "Cannot transfer money to oneself"));
    }

    // Check whether the transfer-out account exists and has sufficient balance
    let wallet_rs = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT user_id, balance FROM wallets WHERE user_id = ?;"),
        sql_params!(&from_user_id),
    )?;

    let from_wallet = if let Some(row) = wallet_rs.next_record()? {
        row
    } else {
        return Err(m_error!(MuduError, "no such user"));
    };

    if from_wallet.get_balance().as_ref().unwrap().get_value() < amount {
        return Err(m_error!(MuduError, "insufficient funds"));
    }

    // Check the user account existing
    let to_wallet = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT user_id FROM wallets WHERE user_id = ?;"),
        sql_params!(&(to_user_id)),
    )?;
    let _to_wallet = if let Some(row) = to_wallet.next_record()? {
        row
    } else {
        return Err(m_error!(MuduError, "no such user"));
    };

    // Perform a transfer operation
    // 1. Deduct the balance of the account transferred out
    let deduct_updated_rows = mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ? WHERE user_id = ?;"),
        sql_params!(&(amount, from_user_id)),
    )?;
    if deduct_updated_rows != 1 {
        return Err(m_error!(MuduError, "transfer fund failed"));
    }
    // 2. Increase the balance of the transfer-in account
    let increase_updated_rows = mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance + ? WHERE user_id = ?;"),
        sql_params!(&(amount, to_user_id)),
    )?;
    if increase_updated_rows != 1 {
        return Err(m_error!(MuduError, "transfer fund failed"));
    }

    // 3. Record the transaction
    let id = Uuid::new_v4().to_string();
    let insert_rows = mudu_command(
        xid,
        sql_stmt!(
            &r#"
        INSERT INTO transactions
        (trans_id, from_user, to_user, amount)
        VALUES (?, ?, ?, ?);
        "#
        ),
        sql_params!(&(id, from_user_id, to_user_id, amount)),
    )?;
    if insert_rows != 1 {
        return Err(m_error!(MuduError, "transfer fund failed"));
    }
    Ok(())
}

#[mudu_proc]
pub fn create_user(xid: XID, user_id: i32, name: String, email: String) -> RS<()> {
    let now = current_timestamp();

    // Insert user
    let user_created = mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
        ),
        sql_params!(&(user_id, name, email, now, now)),
    )?;

    if user_created != 1 {
        return Err(m_error!(MuduError, "Failed to create user"));
    }

    // Create wallet with 0 balance
    let wallet_created = mudu_command(
        xid,
        sql_stmt!(&"INSERT INTO wallets (user_id, balance) VALUES (?, ?)"),
        sql_params!(&(user_id, 0)),
    )?;

    if wallet_created != 1 {
        return Err(m_error!(MuduError, "Failed to create wallet"));
    }

    Ok(())
}

#[mudu_proc]
pub fn delete_user(xid: XID, user_id: i32) -> RS<()> {
    // Check wallet balance
    let wallet_rs = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_params!(&user_id),
    )?;

    let wallet = wallet_rs
        .next_record()?
        .ok_or(m_error!(MuduError, "User wallet not found"))?;

    if wallet.get_balance().as_ref().unwrap().get_value() != 0 {
        return Err(m_error!(
            MuduError,
            "Cannot delete user with non-zero balance"
        ));
    }

    // Delete wallet
    mudu_command(
        xid,
        sql_stmt!(&"DELETE FROM wallets WHERE user_id = ?"),
        sql_params!(&(user_id,)),
    )?;

    // Delete user
    mudu_command(
        xid,
        sql_stmt!(&"DELETE FROM users WHERE user_id = ?"),
        sql_params!(&(user_id,)),
    )?;

    Ok(())
}

#[mudu_proc]
pub fn update_user(xid: XID, user_id: i32, name: String, email: String) -> RS<()> {
    let now = current_timestamp();
    let mut params: Vec<Box<dyn DatumDyn>> = vec![];

    let mut sql = "UPDATE users SET updated_at = ?".to_string();
    params.push(Box::new(now));

    if !name.is_empty() {
        sql += ", name = ?";
        params.push(Box::new(name.clone()));
    }

    if !email.is_empty() {
        sql += ", email = ?";
        params.push(Box::new(email.clone()));
    }

    sql += " WHERE user_id = ?";
    params.push(Box::new(user_id));

    let updated = mudu_command(xid, sql_stmt!(&sql), sql_params!(&params))?;

    if updated != 1 {
        return Err(m_error!(MuduError, "User not found"));
    }

    Ok(())
}

#[mudu_proc]
pub fn deposit(xid: XID, user_id: i32, amount: i32) -> RS<()> {
    if amount <= 0 {
        return Err(m_error!(MuduError, "Amount must be positive"));
    }

    let now = current_timestamp();
    let tx_id = Uuid::new_v4().to_string();

    // Update wallet balance
    let updated = mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance + ?, updated_at = ? WHERE user_id = ?"),
        sql_params!(&(amount, now, user_id)),
    )?;

    if updated != 1 {
        return Err(m_error!(MuduError, "User wallet not found"));
    }

    // Record transaction
    mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO transactions (transaction_id, type, to_user_id, amount, created_at) VALUES (?, ?, ?, ?, ?)"
        ),
        sql_params!(&(tx_id, "DEPOSIT".to_string(), user_id, amount, now)),
    )?;

    Ok(())
}

#[mudu_proc]
pub fn withdraw(xid: XID, user_id: i32, amount: i32) -> RS<()> {
    if amount <= 0 {
        return Err(m_error!(MuduError, "Amount must be positive"));
    }

    // Check balance
    let wallet_rs = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_params!(&user_id),
    )?;

    let wallet = wallet_rs
        .next_record()?
        .ok_or_else(|| m_error!(MuduError, "User wallet not found"))?;

    if wallet.get_balance().as_ref().unwrap().get_value() < amount {
        return Err(m_error!(MuduError, "Insufficient funds"));
    }

    let now = current_timestamp();
    let tx_id = Uuid::new_v4().to_string();

    // Update wallet balance
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ?, updated_at = ? WHERE user_id = ?"),
        sql_params!(&(amount, now, user_id)),
    )?;

    // Record transaction
    mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO transactions (transaction_id, type, from_user_id, amount, created_at) VALUES (?, ?, ?, ?, ?)"
        ),
        sql_params!(&(tx_id, "WITHDRAW".to_string(), user_id, amount, now)),
    )?;

    Ok(())
}

#[mudu_proc]
pub fn transfer(xid: XID, from_user_id: i32, to_user_id: i32, amount: i32) -> RS<()> {
    if from_user_id == to_user_id {
        return Err(m_error!(MuduError, "Cannot transfer to self"));
    }

    if amount <= 0 {
        return Err(m_error!(MuduError, "Amount must be positive"));
    }

    // Check sender balance
    let sender_wallet = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_params!(&from_user_id),
    )?
    .next_record()?
    .ok_or_else(|| m_error!(MuduError, "Sender wallet not found"))?;

    if sender_wallet.get_balance().as_ref().unwrap().get_value() < amount {
        return Err(m_error!(MuduError, "Insufficient funds"));
    }

    // Check receiver exists
    let receiver_exists = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT user_id FROM wallets WHERE user_id = ?"),
        sql_params!(&to_user_id),
    )?
    .next_record()?
    .is_some();

    if !receiver_exists {
        return Err(m_error!(MuduError, "Receiver wallet not found"));
    }

    let now = current_timestamp();
    let tx_id = Uuid::new_v4().to_string();

    // Debit sender
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ?, updated_at = ? WHERE user_id = ?"),
        sql_params!(&(amount, now, from_user_id)),
    )?;

    // Credit receiver
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance + ?, updated_at = ? WHERE user_id = ?"),
        sql_params!(&(amount, now, to_user_id)),
    )?;

    // Record transaction
    mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO transactions (trans_id, trans_type, from_user, to_user, amount, created_at) VALUES (?, ?, ?, ?, ?, ?)"
        ),
        sql_params!(&(
            tx_id,
            "TRANSFER".to_string(),
            from_user_id,
            to_user_id,
            amount,
            now
        )),
    )?;

    Ok(())
}

#[mudu_proc]
pub fn purchase(xid: XID, user_id: i32, amount: i32, description: String) -> RS<()> {
    if amount <= 0 {
        return Err(m_error!(MuduError, "Amount must be positive"));
    }

    // Check balance
    let wallet = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_params!(&user_id),
    )?
    .next_record()?
    .ok_or_else(|| m_error!(MuduError, "Wallet not found"))?;

    if wallet.get_balance().as_ref().unwrap().get_value() < amount {
        return Err(m_error!(MuduError, "Insufficient funds"));
    }

    let now = current_timestamp();
    let tx_id = Uuid::new_v4().to_string();

    // Deduct amount
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ?, updated_at = ? WHERE user_id = ?"),
        sql_params!(&(amount, now, user_id)),
    )?;

    // Record transaction
    mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO transactions (transaction_id, type, from_user_id, amount, description, created_at) VALUES (?, ?, ?, ?, ?, ?)"
        ),
        sql_params!(&(
            tx_id,
            "PURCHASE".to_string(),
            user_id,
            amount,
            description,
            now
        )),
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::rust::procedures::*;
    use mudu::procedure::proc_desc::ProcDesc;
    use mudu::utils::app_proc_desc::AppProcDesc;
    use mudu::utils::toml::to_toml_str;

    #[test]
    fn test_gen_proc_desc() {
        _test_gen_proc_desc().unwrap();
    }
    fn _test_gen_proc_desc() -> RS<()> {
        let vec = vec![
            mudu_proc_desc_deposit(),
            mudu_proc_desc_transfer(),
            mudu_proc_desc_purchase(),
            mudu_proc_desc_create_user(),
            mudu_proc_desc_delete_user(),
            mudu_proc_desc_transfer_funds(),
            mudu_proc_desc_update_user(),
            mudu_proc_desc_withdraw(),
        ]
        .iter()
        .map(|e| (*e).clone())
        .collect::<Vec<ProcDesc>>();
        let mut app_proc_desc = AppProcDesc::new();
        for desc in vec {
            app_proc_desc.add(desc);
        }
        let toml_str = to_toml_str(&app_proc_desc)?;
        println!("{}", toml_str);
        Ok(())
    }
}
