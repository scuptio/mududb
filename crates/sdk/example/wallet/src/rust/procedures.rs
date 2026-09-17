//! Wallet business logic and stored procedures.

use crate::rust::users::object::{Users, UsersChange};
use crate::rust::wallets::object::{Wallets, WalletsChange};
use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::contract::database::field_change::FieldChange;
use mududb::contract::{sql_params, sql_stmt};
use mududb::error::ErrorCode;
use mududb::mudu_error;
use mududb::sys_interface::sync_api::{mudu_command, mudu_query};
use std::time::UNIX_EPOCH;

/// Returns the current Unix timestamp in seconds.
fn current_timestamp() -> RS<i64> {
    let now = mududb::sys::time::system_time_now();
    let duration_since_epoch = now
        .duration_since(UNIX_EPOCH)
        .map_err(|_| mudu_error!(ErrorCode::DomainViolation, "SystemTime before UNIX EPOCH!"))?;

    let seconds = duration_since_epoch.as_secs();
    Ok(seconds as _)
}

/// Returns the wallet's balance, failing if the balance column is `NULL`.
fn required_balance(wallet: &Wallets) -> RS<i32> {
    wallet
        .balance
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidState, "wallet balance is null"))
}

/// Queries a wallet by primary key, failing if no such row exists.
fn query_wallet(xid: OID, user_id: i32, not_found: &str) -> RS<Wallets> {
    mudu_query::<Wallets>(
        xid,
        sql_stmt!(&Wallets::SQL_GET_BY_PK),
        sql_params!(&(user_id,)),
    )?
    .next_record()?
    .ok_or_else(|| mudu_error!(ErrorCode::EntityNotFound, not_found))
}

/// Applies a partial wallet update; exactly one row must be affected.
fn update_wallet(xid: OID, update: &WalletsChange, user_id: i32) -> RS<()> {
    let (sql, params) = update
        .update_by_pk(user_id)
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "empty wallet update"))?;
    let updated = mudu_command(xid, sql_stmt!(&sql), sql_params!(&params))?;
    if updated != 1 {
        return Err(mudu_error!(
            ErrorCode::DomainViolation,
            "transfer fund failed"
        ));
    }
    Ok(())
}

/**mudu-proc**/
/// Transfers `amount` funds from `from_user_id` to `to_user_id`.
pub fn transfer_funds(xid: OID, from_user_id: i32, to_user_id: i32, amount: i32) -> RS<()> {
    // Check amount > 0
    if amount <= 0 {
        return Err(mudu_error!(
            ErrorCode::DomainViolation,
            "The transfer amount must be greater than 0"
        ));
    }

    // Cannot transfer money to oneself
    if from_user_id == to_user_id {
        return Err(mudu_error!(
            ErrorCode::DomainViolation,
            "Cannot transfer money to oneself"
        ));
    }

    // Check whether the transfer-out account exists and has sufficient balance
    let from_wallet = query_wallet(xid, from_user_id, "no such user")?;
    let from_balance = required_balance(&from_wallet)?;
    if from_balance < amount {
        return Err(mudu_error!(
            ErrorCode::DomainViolation,
            "insufficient funds"
        ));
    }

    // Check the user account existing
    let to_wallet = query_wallet(xid, to_user_id, "no such user")?;
    let to_balance = required_balance(&to_wallet)?;

    // Perform a transfer operation
    // 1. Deduct the balance of the account transferred out
    update_wallet(
        xid,
        &WalletsChange {
            balance: FieldChange::Set(Some(from_balance - amount)),
            ..WalletsChange::default()
        },
        from_user_id,
    )?;
    // 2. Increase the balance of the transfer-in account
    update_wallet(
        xid,
        &WalletsChange {
            balance: FieldChange::Set(Some(to_balance + amount)),
            ..WalletsChange::default()
        },
        to_user_id,
    )?;

    // 3. Entity the transaction
    let id = mududb::sys::random::next_uuid_v4_string();
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
        return Err(mudu_error!(
            ErrorCode::DomainViolation,
            "transfer fund failed"
        ));
    }
    Ok(())
}

/**mudu-proc**/
/// Creates a new user and an associated wallet with a zero balance.
pub fn create_user(xid: OID, user_id: i32, name: String, email: String) -> RS<()> {
    let now = current_timestamp()?;

    // Insert user
    let user_created = mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
        ),
        sql_params!(&(user_id, name, email, now, now)),
    )?;

    if user_created != 1 {
        return Err(mudu_error!(
            ErrorCode::DomainViolation,
            "Failed to create user"
        ));
    }

    // Create wallet with 0 balance, bound through the entity's typed
    // insert parameters.
    let wallet = Wallets::new(user_id, Some(0), Some(now as i32));
    let wallet_created = mudu_command(
        xid,
        sql_stmt!(&Wallets::SQL_INSERT),
        sql_params!(&wallet.insert_params()),
    )?;

    if wallet_created != 1 {
        return Err(mudu_error!(
            ErrorCode::DomainViolation,
            "Failed to create wallet"
        ));
    }

    Ok(())
}

/**mudu-proc**/
/// Deletes a user and their wallet if the wallet balance is zero.
pub fn delete_user(xid: OID, user_id: i32) -> RS<()> {
    // Check wallet balance
    let wallet = query_wallet(xid, user_id, "User wallet not found")?;

    let balance = required_balance(&wallet)?;
    if balance != 0 {
        return Err(mudu_error!(
            ErrorCode::DomainViolation,
            "Cannot delete user with non-zero balance"
        ));
    }

    // Delete wallet
    mudu_command(
        xid,
        sql_stmt!(&Wallets::SQL_DELETE_BY_PK),
        sql_params!(&(user_id,)),
    )?;

    // Delete user
    mudu_command(
        xid,
        sql_stmt!(&Users::SQL_DELETE_BY_PK),
        sql_params!(&(user_id,)),
    )?;

    Ok(())
}

/**mudu-proc**/
/// Updates a user's name and/or email.
pub fn update_user(xid: OID, user_id: i32, name: String, email: String) -> RS<()> {
    let now = current_timestamp()?;
    let update = UsersChange {
        name: if name.is_empty() {
            FieldChange::Unchanged
        } else {
            FieldChange::Set(Some(name))
        },
        email: if email.is_empty() {
            FieldChange::Unchanged
        } else {
            FieldChange::Set(Some(email))
        },
        updated_at: FieldChange::Set(Some(now as i32)),
        ..UsersChange::default()
    };
    let (sql, params) = update
        .update_by_pk(user_id)
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "empty user update"))?;

    let updated = mudu_command(xid, sql_stmt!(&sql), sql_params!(&params))?;

    if updated != 1 {
        return Err(mudu_error!(ErrorCode::EntityNotFound, "User not found"));
    }

    Ok(())
}

/**mudu-proc**/
/// Deposits `amount` funds into the user's wallet.
pub fn deposit(xid: OID, user_id: i32, amount: i32) -> RS<()> {
    if amount <= 0 {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "Amount must be positive"
        ));
    }

    let now = current_timestamp()?;
    let tx_id = mududb::sys::random::next_uuid_v4_string();
    let wallet = query_wallet(xid, user_id, "User wallet not found")?;
    let next_balance = required_balance(&wallet)? + amount;

    // Update wallet balance
    update_wallet(
        xid,
        &WalletsChange {
            balance: FieldChange::Set(Some(next_balance)),
            updated_at: FieldChange::Set(Some(now as i32)),
        },
        user_id,
    )?;

    // Entity transaction
    mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO transactions (trans_id, trans_type, to_user, amount, created_at) VALUES (?, ?, ?, ?, ?)"
        ),
        sql_params!(&(tx_id, "DEPOSIT".to_string(), user_id, amount, now)),
    )?;

    Ok(())
}

/**mudu-proc**/
/// Withdraws `amount` funds from the user's wallet.
pub fn withdraw(xid: OID, user_id: i32, amount: i32) -> RS<()> {
    if amount <= 0 {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "Amount must be positive"
        ));
    }

    // Check balance
    let wallet = query_wallet(xid, user_id, "User wallet not found")?;
    let balance = required_balance(&wallet)?;
    if balance < amount {
        return Err(mudu_error!(
            ErrorCode::DomainViolation,
            "Insufficient funds"
        ));
    }

    let now = current_timestamp()?;
    let tx_id = mududb::sys::random::next_uuid_v4_string();
    let next_balance = balance - amount;

    // Update wallet balance
    update_wallet(
        xid,
        &WalletsChange {
            balance: FieldChange::Set(Some(next_balance)),
            updated_at: FieldChange::Set(Some(now as i32)),
        },
        user_id,
    )?;

    // Entity transaction
    mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO transactions (trans_id, trans_type, from_user, amount, created_at) VALUES (?, ?, ?, ?, ?)"
        ),
        sql_params!(&(tx_id, "WITHDRAW".to_string(), user_id, amount, now)),
    )?;

    Ok(())
}

/**mudu-proc**/
/// Transfers `amount` funds from `from_user_id` to `to_user_id`.
pub fn transfer(xid: OID, from_user_id: i32, to_user_id: i32, amount: i32) -> RS<()> {
    if from_user_id == to_user_id {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "Cannot transfer to self"
        ));
    }

    if amount <= 0 {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "Amount must be positive"
        ));
    }

    // Check sender balance
    let sender_wallet = query_wallet(xid, from_user_id, "Sender wallet not found")?;
    let sender_balance = required_balance(&sender_wallet)?;
    if sender_balance < amount {
        return Err(mudu_error!(
            ErrorCode::DomainViolation,
            "Insufficient funds"
        ));
    }

    // Check receiver exists
    let receiver_wallet = query_wallet(xid, to_user_id, "Receiver wallet not found")?;
    let receiver_balance = required_balance(&receiver_wallet)?;

    let now = current_timestamp()?;
    let tx_id = mududb::sys::random::next_uuid_v4_string();

    // Debit sender
    update_wallet(
        xid,
        &WalletsChange {
            balance: FieldChange::Set(Some(sender_balance - amount)),
            updated_at: FieldChange::Set(Some(now as i32)),
        },
        from_user_id,
    )?;

    // Credit receiver
    update_wallet(
        xid,
        &WalletsChange {
            balance: FieldChange::Set(Some(receiver_balance + amount)),
            updated_at: FieldChange::Set(Some(now as i32)),
        },
        to_user_id,
    )?;

    // Entity transaction
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

/**mudu-proc**/
/// Records a purchase of `amount` funds by the user.
pub fn purchase(xid: OID, user_id: i32, amount: i32, description: String) -> RS<()> {
    let _ = description;
    if amount <= 0 {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "Amount must be positive"
        ));
    }

    // Check balance
    let wallet = query_wallet(xid, user_id, "Wallet not found")?;
    let balance = required_balance(&wallet)?;
    if balance < amount {
        return Err(mudu_error!(
            ErrorCode::DomainViolation,
            "Insufficient funds"
        ));
    }

    let now = current_timestamp()?;
    let tx_id = mududb::sys::random::next_uuid_v4_string();
    let next_balance = balance - amount;

    // Deduct amount
    update_wallet(
        xid,
        &WalletsChange {
            balance: FieldChange::Set(Some(next_balance)),
            updated_at: FieldChange::Set(Some(now as i32)),
        },
        user_id,
    )?;

    // Entity transaction
    mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO transactions (trans_id, trans_type, from_user, amount, created_at) VALUES (?, ?, ?, ?, ?)"
        ),
        sql_params!(&(tx_id, "PURCHASE".to_string(), user_id, amount, now)),
    )?;

    Ok(())
}
