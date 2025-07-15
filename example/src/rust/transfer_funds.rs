use crate::rust::wallets::object::Wallets;
use mudu::common::error::ER::MuduError;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::database::attribute::Attribute;
use mudu::database::sql::{command, query};
use mudu::tuple::to_datum::ToDatum;
use mudu::{sql_param, sql_stmt};
use mudu_procedure::mudu_procedure;
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

#[mudu_procedure]
pub fn transfer_funds(xid: XID, from_user_id: i32, to_user_id: i32, amount: i32) -> RS<()> {
    // Check amount > 0
    if amount <= 0 {
        return Err(MuduError(
            "The transfer amount must be greater than 0".to_string(),
        ));
    }

    // Cannot transfer money to oneself
    if from_user_id == to_user_id {
        return Err(MuduError("Cannot transfer money to oneself".to_string()));
    }

    // Check whether the transfer-out account exists and has sufficient balance
    let wallet_rs = query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT user_id, balance FROM wallets WHERE user_id = ?;"),
        sql_param!(&[&from_user_id]),
    )?;

    let from_wallet = if let Some(row) = wallet_rs.next()? {
        row
    } else {
        return Err(MuduError("no such user".to_string()));
    };

    if from_wallet.get_balance().as_ref().unwrap().get_value() < amount {
        return Err(MuduError("insufficient funds".to_string()));
    }

    // Check the user account existing
    let to_wallet = query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT user_id FROM wallets WHERE user_id = ?;"),
        sql_param!(&[&to_user_id]),
    )?;
    let _to_wallet = if let Some(row) = to_wallet.next()? {
        row
    } else {
        return Err(MuduError("no such user".to_string()));
    };

    // Perform a transfer operation
    // 1. Deduct the balance of the account transferred out
    let deduct_updated_rows = command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ? WHERE user_id = ?;"),
        sql_param!(&[&amount, &from_user_id]),
    )?;
    if deduct_updated_rows != 1 {
        return Err(MuduError("transfer fund failed".to_string()));
    }
    // 2. Increase the balance of the transfer-in account
    let increase_updated_rows = command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance + ? WHERE user_id = ?;"),
        sql_param!(&[&amount, &to_user_id]),
    )?;
    if increase_updated_rows != 1 {
        return Err(MuduError("transfer fund failed".to_string()));
    }


    // 3. Record the transaction
    let id = Uuid::new_v4().to_string();
    let insert_rows = command(
        xid,
        sql_stmt!(&r#"
        INSERT INTO transactions
        (trans_id, from_user, to_user, amount)
        VALUES (?, ?, ?, ?);
        "#),
        sql_param!(&[&id, &from_user_id, &to_user_id, &amount]),
    )?;
    if insert_rows != 1 {
        return Err(MuduError("transfer fund failed".to_string()));
    }
    Ok(())
}


#[mudu_procedure]
pub fn create_user(
    xid: XID,
    user_id: i32,
    name: String,
    email: String
) -> RS<()> {
    let now = current_timestamp();

    // Insert user
    let user_created = command(
        xid,
        sql_stmt!(&"INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"),
        sql_param!(&[&user_id, &name, &email, &now, &now]),
    )?;

    if user_created != 1 {
        return Err(MuduError("Failed to create user".to_string()));
    }

    // Create wallet with 0 balance
    let wallet_created = command(
        xid,
        sql_stmt!(&"INSERT INTO wallets (user_id, balance, created_at, updated_at) VALUES (?, ?, ?, ?)"),
        sql_param!(&[&user_id, &0, &now, &now]),
    )?;

    if wallet_created != 1 {
        return Err(MuduError("Failed to create wallet".to_string()));
    }

    Ok(())
}



#[mudu_procedure]
pub fn delete_user(xid: XID, user_id: i32) -> RS<()> {
    // Check wallet balance
    let wallet_rs = query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_param!(&[&user_id]),
    )?;

    let wallet = wallet_rs.next()?
        .ok_or(MuduError("User wallet not found".to_string()))?;

    if wallet.get_balance().as_ref().unwrap().get_value() != 0 {
        return Err(MuduError("Cannot delete user with non-zero balance".to_string()));
    }

    // Delete wallet
    command(
        xid,
        sql_stmt!(&"DELETE FROM wallets WHERE user_id = ?"),
        sql_param!(&[&user_id]),
    )?;

    // Delete user
    command(
        xid,
        sql_stmt!(&"DELETE FROM users WHERE user_id = ?"),
        sql_param!(&[&user_id]),
    )?;

    Ok(())
}

#[mudu_procedure]
pub fn update_user(
    xid: XID,
    user_id: i32,
    name: Option<String>,
    email: Option<String>
) -> RS<()> {
    let now = current_timestamp();
    let mut params: Vec<&dyn ToDatum> = vec![];

    let mut sql = "UPDATE users SET updated_at = ?".to_string();
    params.push(&now);

    if let Some(name) = &name {
        sql += ", name = ?";
        params.push(name);
    }

    if let Some(email) = &email {
        sql += ", email = ?";
        params.push(email);
    }

    sql += " WHERE user_id = ?";
    params.push(&user_id);

    let updated = command(
        xid,
        sql_stmt!(&sql),
        sql_param!(&params),
    )?;

    if updated != 1 {
        return Err(MuduError("User not found".to_string()));
    }

    Ok(())
}

#[mudu_procedure]
pub fn deposit(xid: XID, user_id: i32, amount: i32) -> RS<()> {
    if amount <= 0 {
        return Err(MuduError("Amount must be positive".to_string()));
    }

    let now = current_timestamp();
    let tx_id = Uuid::new_v4().to_string();

    // Update wallet balance
    let updated = command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance + ?, updated_at = ? WHERE user_id = ?"),
        sql_param!(&[&amount, &now, &user_id]),
    )?;

    if updated != 1 {
        return Err(MuduError("User wallet not found".to_string()));
    }

    // Record transaction
    command(
        xid,
        sql_stmt!(&"INSERT INTO transactions (transaction_id, type, to_user_id, amount, created_at) VALUES (?, ?, ?, ?, ?)"),
        sql_param!(&[&tx_id, &"DEPOSIT".to_string(), &user_id, &amount, &now]),
    )?;

    Ok(())
}

pub fn withdraw(xid: XID, user_id: i32, amount: i32) -> RS<()> {
    if amount <= 0 {
        return Err(MuduError("Amount must be positive".to_string()));
    }

    // Check balance
    let wallet_rs = query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_param!(&[&user_id]),
    )?;

    let wallet = wallet_rs.next()?
        .ok_or(MuduError("User wallet not found".to_string()))?;

    if wallet.get_balance().as_ref().unwrap().get_value() < amount {
        return Err(MuduError("Insufficient funds".to_string()));
    }

    let now = current_timestamp();
    let tx_id = Uuid::new_v4().to_string();

    // Update wallet balance
    command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ?, updated_at = ? WHERE user_id = ?"),
        sql_param!(&[&amount, &now, &user_id]),
    )?;

    // Record transaction
    command(
        xid,
        sql_stmt!(&"INSERT INTO transactions (transaction_id, type, from_user_id, amount, created_at) VALUES (?, ?, ?, ?, ?)"),
        sql_param!(&[&tx_id, &"WITHDRAW".to_string(), &user_id, &amount, &now]),
    )?;

    Ok(())
}

#[mudu_procedure]
pub fn transfer(
    xid: XID,
    from_user_id: i32,
    to_user_id: i32,
    amount: i32
) -> RS<()> {
    if from_user_id == to_user_id {
        return Err(MuduError("Cannot transfer to self".to_string()));
    }

    if amount <= 0 {
        return Err(MuduError("Amount must be positive".to_string()));
    }

    // Check sender balance
    let sender_wallet = query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_param!(&[&from_user_id]),
    )?
        .next()?
        .ok_or(MuduError("Sender wallet not found".to_string()))?;

    if sender_wallet.get_balance().as_ref().unwrap().get_value() < amount {
        return Err(MuduError("Insufficient funds".to_string()));
    }

    // Check receiver exists
    let receiver_exists = query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT 1 FROM wallets WHERE user_id = ?"),
        sql_param!(&[&to_user_id]),
    )?
        .next()?
        .is_some();

    if !receiver_exists {
        return Err(MuduError("Receiver wallet not found".to_string()));
    }

    let now = current_timestamp();
    let tx_id = Uuid::new_v4().to_string();

    // Debit sender
    command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ?, updated_at = ? WHERE user_id = ?"),
        sql_param!(&[&amount, &now, &from_user_id]),
    )?;

    // Credit receiver
    command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance + ?, updated_at = ? WHERE user_id = ?"),
        sql_param!(&[&amount, &now, &to_user_id]),
    )?;

    // Record transaction
    command(
        xid,
        sql_stmt!(&"INSERT INTO transactions (transaction_id, type, from_user_id, to_user_id, amount, created_at) VALUES (?, ?, ?, ?, ?, ?)"),
        sql_param!(&[&tx_id, &"TRANSFER".to_string(), &from_user_id, &to_user_id, &amount, &now]),
    )?;

    Ok(())
}

#[mudu_procedure]
pub fn purchase(
    xid: XID,
    user_id: i32,
    amount: i32,
    description: String
) -> RS<()> {
    if amount <= 0 {
        return Err(MuduError("Amount must be positive".to_string()));
    }

    // Check balance
    let wallet = query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_param!(&[&user_id]),
    )?
        .next()?
        .ok_or(MuduError("Wallet not found".to_string()))?;

    if wallet.get_balance().as_ref().unwrap().get_value() < amount {
        return Err(MuduError("Insufficient funds".to_string()));
    }

    let now = current_timestamp();
    let tx_id = Uuid::new_v4().to_string();

    // Deduct amount
    command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ?, updated_at = ? WHERE user_id = ?"),
        sql_param!(&[&amount, &now, &user_id]),
    )?;

    // Record transaction
    command(
        xid,
        sql_stmt!(&"INSERT INTO transactions (transaction_id, type, from_user_id, amount, description, created_at) VALUES (?, ?, ?, ?, ?, ?)"),
        sql_param!(&[&tx_id, &"PURCHASE".to_string(), &user_id, &amount, &description, &now]),
    )?;

    Ok(())
}