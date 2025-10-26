use crate::rust::wallets::object::Wallets;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::database::attr_value::AttrValue;
use mudu::database::sql::{command, query};
use mudu::error::ec::EC::MuduError;
use mudu::tuple::datum::DatumDyn;
use mudu::{m_error, sql_params, sql_stmt};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

fn current_timestamp() -> i64 {
    let now = SystemTime::now();
    let duration_since_epoch = now
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime before UNIX EPOCH!");

    let seconds = duration_since_epoch.as_secs();
    seconds as _
}

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
    let wallet_rs = query::<Wallets>(
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
    let to_wallet = query::<Wallets>(
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
    let deduct_updated_rows = command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ? WHERE user_id = ?;"),
        sql_params!(&(amount, from_user_id)),
    )?;
    if deduct_updated_rows != 1 {
        return Err(m_error!(MuduError, "transfer fund failed"));
    }
    // 2. Increase the balance of the transfer-in account
    let increase_updated_rows = command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance + ? WHERE user_id = ?;"),
        sql_params!(&(amount, to_user_id)),
    )?;
    if increase_updated_rows != 1 {
        return Err(m_error!(MuduError, "transfer fund failed"));
    }

    // 3. Record the transaction
    let id = Uuid::new_v4().to_string();
    let insert_rows = command(
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

pub fn create_user(xid: XID, user_id: i32, name: String, email: String) -> RS<()> {
    let now = current_timestamp();

    // Insert user
    let user_created = command(
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
    let wallet_created = command(
        xid,
        sql_stmt!(
            &"INSERT INTO wallets (user_id, balance, created_at, updated_at) VALUES (?, ?, ?, ?)"
        ),
        sql_params!(&(user_id, 0, now, now)),
    )?;

    if wallet_created != 1 {
        return Err(m_error!(MuduError, "Failed to create wallet"));
    }

    Ok(())
}

pub fn delete_user(xid: XID, user_id: i32) -> RS<()> {
    // Check wallet balance
    let wallet_rs = query::<Wallets>(
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
    command(
        xid,
        sql_stmt!(&"DELETE FROM wallets WHERE user_id = ?"),
        sql_params!(&(user_id,)),
    )?;

    // Delete user
    command(
        xid,
        sql_stmt!(&"DELETE FROM users WHERE user_id = ?"),
        sql_params!(&(user_id,)),
    )?;

    Ok(())
}

pub fn update_user(xid: XID, user_id: i32, name: Option<String>, email: Option<String>) -> RS<()> {
    let now = current_timestamp();
    let mut params: Vec<Box<dyn DatumDyn>> = vec![];

    let mut sql = "UPDATE users SET updated_at = ?".to_string();
    params.push(Box::new(now));

    if let Some(name) = &name {
        sql += ", name = ?";
        params.push(Box::new(name.clone()));
    }

    if let Some(email) = &email {
        sql += ", email = ?";
        params.push(Box::new(email.clone()));
    }

    sql += " WHERE user_id = ?";
    params.push(Box::new(user_id));

    let updated = command(xid, sql_stmt!(&sql), sql_params!(&params))?;

    if updated != 1 {
        return Err(m_error!(MuduError, "User not found"));
    }

    Ok(())
}

pub fn deposit(xid: XID, user_id: i32, amount: i32) -> RS<()> {
    if amount <= 0 {
        return Err(m_error!(MuduError, "Amount must be positive"));
    }

    let now = current_timestamp();
    let tx_id = Uuid::new_v4().to_string();

    // Update wallet balance
    let updated = command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance + ?, updated_at = ? WHERE user_id = ?"),
        sql_params!(&(amount, now, user_id)),
    )?;

    if updated != 1 {
        return Err(m_error!(MuduError, "User wallet not found"));
    }

    // Record transaction
    command(
        xid,
        sql_stmt!(
            &"INSERT INTO transactions (transaction_id, type, to_user_id, amount, created_at) VALUES (?, ?, ?, ?, ?)"
        ),
        sql_params!(&(tx_id, "DEPOSIT".to_string(), user_id, amount, now)),
    )?;

    Ok(())
}

pub fn withdraw(xid: XID, user_id: i32, amount: i32) -> RS<()> {
    if amount <= 0 {
        return Err(m_error!(MuduError, "Amount must be positive"));
    }

    // Check balance
    let wallet_rs = query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_params!(&user_id),
    )?;

    let wallet = wallet_rs
        .next_record()?
        .ok_or(m_error!(MuduError, "User wallet not found"))?;

    if wallet.get_balance().as_ref().unwrap().get_value() < amount {
        return Err(m_error!(MuduError, "Insufficient funds"));
    }

    let now = current_timestamp();
    let tx_id = Uuid::new_v4().to_string();

    // Update wallet balance
    command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ?, updated_at = ? WHERE user_id = ?"),
        sql_params!(&(amount, now, user_id)),
    )?;

    // Record transaction
    command(
        xid,
        sql_stmt!(
            &"INSERT INTO transactions (transaction_id, type, from_user_id, amount, created_at) VALUES (?, ?, ?, ?, ?)"
        ),
        sql_params!(&(tx_id, "WITHDRAW".to_string(), user_id, amount, now)),
    )?;

    Ok(())
}

pub fn transfer(xid: XID, from_user_id: i32, to_user_id: i32, amount: i32) -> RS<()> {
    if from_user_id == to_user_id {
        return Err(m_error!(MuduError, "Cannot transfer to self"));
    }

    if amount <= 0 {
        return Err(m_error!(MuduError, "Amount must be positive"));
    }

    // Check sender balance
    let sender_wallet = query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_params!(&from_user_id),
    )?
    .next_record()?
    .ok_or(m_error!(MuduError, "Sender wallet not found"))?;

    if sender_wallet.get_balance().as_ref().unwrap().get_value() < amount {
        return Err(m_error!(MuduError, "Insufficient funds"));
    }

    // Check receiver exists
    let receiver_exists = query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT 1 FROM wallets WHERE user_id = ?"),
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
    command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ?, updated_at = ? WHERE user_id = ?"),
        sql_params!(&(amount, now, from_user_id)),
    )?;

    // Credit receiver
    command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance + ?, updated_at = ? WHERE user_id = ?"),
        sql_params!(&(amount, now, to_user_id)),
    )?;

    // Record transaction
    command(
        xid,
        sql_stmt!(
            &"INSERT INTO transactions (transaction_id, type, from_user_id, to_user_id, amount, created_at) VALUES (?, ?, ?, ?, ?, ?)"
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

pub fn purchase(xid: XID, user_id: i32, amount: i32, description: String) -> RS<()> {
    if amount <= 0 {
        return Err(m_error!(MuduError, "Amount must be positive"));
    }

    // Check balance
    let wallet = query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_params!(&user_id),
    )?
    .next_record()?
    .ok_or(m_error!(MuduError, "Wallet not found"))?;

    if wallet.get_balance().as_ref().unwrap().get_value() < amount {
        return Err(m_error!(MuduError, "Insufficient funds"));
    }

    let now = current_timestamp();
    let tx_id = Uuid::new_v4().to_string();

    // Deduct amount
    command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ?, updated_at = ? WHERE user_id = ?"),
        sql_params!(&(amount, now, user_id)),
    )?;

    // Record transaction
    command(
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
