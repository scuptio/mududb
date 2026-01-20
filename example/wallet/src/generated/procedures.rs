use crate::generated::wallets::object::Wallets;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::error::ec::EC::MuduError;
use mudu::m_error;
use mudu_contract::database::attr_value::AttrValue;
use mudu_contract::{sql_params, sql_stmt};
use mudu_type::datum::DatumDyn;
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

/**mudu-proc**/
pub async fn transfer_funds(xid: XID, from_user_id: i32, to_user_id: i32, amount: i32) -> RS<()> {
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
        sql_params!(&(from_user_id, )),
    ).await?;

    let from_wallet = if let Some(row) = wallet_rs.next_record()? {
        row
    } else {
        return Err(m_error!(MuduError, "no such user"));
    };

    if *from_wallet.get_balance().as_ref().unwrap() < amount {
        return Err(m_error!(MuduError, "insufficient funds"));
    }

    // Check the user account existing
    let to_wallet = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT user_id FROM wallets WHERE user_id = ?;"),
        sql_params!(&(to_user_id)),
    ).await?;
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
    ).await?;
    if deduct_updated_rows != 1 {
        return Err(m_error!(MuduError, "transfer fund failed"));
    }
    // 2. Increase the balance of the transfer-in account
    let increase_updated_rows = mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance + ? WHERE user_id = ?;"),
        sql_params!(&(amount, to_user_id)),
    ).await?;
    if increase_updated_rows != 1 {
        return Err(m_error!(MuduError, "transfer fund failed"));
    }

    // 3. Entity the transaction
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
    ).await?;
    if insert_rows != 1 {
        return Err(m_error!(MuduError, "transfer fund failed"));
    }
    Ok(())
}

/**mudu-proc**/
pub async fn create_user(xid: XID, user_id: i32, name: String, email: String) -> RS<()> {
    let now = current_timestamp();

    // Insert user
    let user_created = mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
        ),
        sql_params!(&(user_id, name, email, now, now)),
    ).await?;

    if user_created != 1 {
        return Err(m_error!(MuduError, "Failed to create user"));
    }

    // Create wallet with 0 balance
    let wallet_created = mudu_command(
        xid,
        sql_stmt!(&"INSERT INTO wallets (user_id, balance) VALUES (?, ?)"),
        sql_params!(&(user_id, 0)),
    ).await?;

    if wallet_created != 1 {
        return Err(m_error!(MuduError, "Failed to create wallet"));
    }

    Ok(())
}

/**mudu-proc**/
pub async fn delete_user(xid: XID, user_id: i32) -> RS<()> {
    // Check wallet balance
    let wallet_rs = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_params!(&(user_id, )),
    ).await?;

    let wallet = wallet_rs
        .next_record()?
        .ok_or(m_error!(MuduError, "User wallet not found"))?;

    if *wallet.get_balance().as_ref().unwrap() != 0 {
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
    ).await?;

    // Delete user
    mudu_command(
        xid,
        sql_stmt!(&"DELETE FROM users WHERE user_id = ?"),
        sql_params!(&(user_id,)),
    ).await?;

    Ok(())
}

/**mudu-proc**/
pub async fn update_user(xid: XID, user_id: i32, name: String, email: String) -> RS<()> {
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

    let updated = mudu_command(xid, sql_stmt!(&sql), sql_params!(&params)).await?;

    if updated != 1 {
        return Err(m_error!(MuduError, "User not found"));
    }

    Ok(())
}

/**mudu-proc**/
pub async fn deposit(xid: XID, user_id: i32, amount: i32) -> RS<()> {
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
    ).await?;

    if updated != 1 {
        return Err(m_error!(MuduError, "User wallet not found"));
    }

    // Entity transaction
    mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO transactions (transaction_id, type, to_user_id, amount, created_at) VALUES (?, ?, ?, ?, ?)"
        ),
        sql_params!(&(tx_id, "DEPOSIT".to_string(), user_id, amount, now)),
    ).await?;

    Ok(())
}

/**mudu-proc**/
pub async fn withdraw(xid: XID, user_id: i32, amount: i32) -> RS<()> {
    if amount <= 0 {
        return Err(m_error!(MuduError, "Amount must be positive"));
    }

    // Check balance
    let wallet_rs = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_params!(&(user_id, )),
    ).await?;

    let wallet = wallet_rs
        .next_record()?
        .ok_or_else(|| m_error!(MuduError, "User wallet not found"))?;

    if *wallet.get_balance().as_ref().unwrap() < amount {
        return Err(m_error!(MuduError, "Insufficient funds"));
    }

    let now = current_timestamp();
    let tx_id = Uuid::new_v4().to_string();

    // Update wallet balance
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ?, updated_at = ? WHERE user_id = ?"),
        sql_params!(&(amount, now, user_id)),
    ).await?;

    // Entity transaction
    mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO transactions (transaction_id, type, from_user_id, amount, created_at) VALUES (?, ?, ?, ?, ?)"
        ),
        sql_params!(&(tx_id, "WITHDRAW".to_string(), user_id, amount, now)),
    ).await?;

    Ok(())
}

/**mudu-proc**/
pub async fn transfer(xid: XID, from_user_id: i32, to_user_id: i32, amount: i32) -> RS<()> {
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
        sql_params!(&(from_user_id, )),
    ).await?
    .next_record()?
    .ok_or_else(|| m_error!(MuduError, "Sender wallet not found"))?;

    if *sender_wallet.get_balance().as_ref().unwrap() < amount {
        return Err(m_error!(MuduError, "Insufficient funds"));
    }

    // Check receiver exists
    let receiver_exists = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT user_id FROM wallets WHERE user_id = ?"),
        sql_params!(&(to_user_id.clone(), )),
    ).await?
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
    ).await?;

    // Credit receiver
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance + ?, updated_at = ? WHERE user_id = ?"),
        sql_params!(&(amount, now, to_user_id)),
    ).await?;

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
    ).await?;

    Ok(())
}

/**mudu-proc**/
pub async fn purchase(xid: XID, user_id: i32, amount: i32, description: String) -> RS<()> {
    if amount <= 0 {
        return Err(m_error!(MuduError, "Amount must be positive"));
    }

    // Check balance
    let wallet = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_params!(&(user_id,)),
    ).await?
    .next_record()?
    .ok_or_else(|| m_error!(MuduError, "Wallet not found"))?;

    if *wallet.get_balance().as_ref().unwrap() < amount {
        return Err(m_error!(MuduError, "Insufficient funds"));
    }

    let now = current_timestamp();
    let tx_id = Uuid::new_v4().to_string();

    // Deduct amount
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ?, updated_at = ? WHERE user_id = ?"),
        sql_params!(&(amount, now, user_id)),
    ).await?;

    // Entity transaction
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
    ).await?;

    Ok(())
}
async fn mp2_purchase(param: Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_purchase,
    ).await
}

pub async fn mudu_inner_p2_purchase(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_purchase().clone();
    let res = purchase(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
        
            ::mudu_type::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[2], "String")?,
        
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_purchase() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                
                    i32,
                    i32,
                
                    String,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                            
                                "user_id",
                                "amount",
                                "description",
                            

                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_purchase() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_purchase() -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "purchase".to_string(),
                mudu_argv_desc_purchase().clone(),
                mudu_result_desc_purchase().clone(),
                false
            )
        })
}

mod mod_purchase {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-purchase;
            world mudu-app-mp2-purchase {
                export mp2-purchase: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestPurchase {}

    impl Guest for GuestPurchase {
        async fn mp2_purchase(param: Vec<u8>) -> Vec<u8> {
            super::mp2_purchase(param).await
        }
    }

    export!(GuestPurchase);
}
async fn mp2_withdraw(param: Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_withdraw,
    ).await
}

pub async fn mudu_inner_p2_withdraw(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_withdraw().clone();
    let res = withdraw(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
        
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_withdraw() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                
                    i32,
                
                    i32,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                            
                                "user_id",
                            
                                "amount",
                            

                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_withdraw() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_withdraw() -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "withdraw".to_string(),
                mudu_argv_desc_withdraw().clone(),
                mudu_result_desc_withdraw().clone(),
                false
            )
        })
}

mod mod_withdraw {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-withdraw;
            world mudu-app-mp2-withdraw {
                export mp2-withdraw: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestWithdraw {}

    impl Guest for GuestWithdraw {
        async fn mp2_withdraw(param: Vec<u8>) -> Vec<u8> {
            super::mp2_withdraw(param).await
        }
    }

    export!(GuestWithdraw);
}
async fn mp2_create_user(param: Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_create_user,
    ).await
}

pub async fn mudu_inner_p2_create_user(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_create_user().clone();
    let res = create_user(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
        
            ::mudu_type::datum::value_to_typed::<
                String,
                _,
            >(&param.param_list()[1], "String")?,
        ::mudu_type::datum::value_to_typed::<
            String,
            _,
        >(&param.param_list()[2], "String")?,
        
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_create_user() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                
                    i32,
                    String,
                    String,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                            
                                "user_id",
                                "name",
                                "email",
                            

                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_create_user() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_create_user() -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "create_user".to_string(),
                mudu_argv_desc_create_user().clone(),
                mudu_result_desc_create_user().clone(),
                false
            )
        })
}

mod mod_create_user {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-create-user;
            world mudu-app-mp2-create-user {
                export mp2-create-user: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestCreateUser {}

    impl Guest for GuestCreateUser {
        async fn mp2_create_user(param: Vec<u8>) -> Vec<u8> {
            super::mp2_create_user(param).await
        }
    }

    export!(GuestCreateUser);
}
async fn mp2_transfer(param: Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_transfer,
    ).await
}

pub async fn mudu_inner_p2_transfer(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_transfer().clone();
    let res = transfer(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
        ::mudu_type::datum::value_to_typed::<
            i32,
            _,
        >(&param.param_list()[2], "i32")?,
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_transfer() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                
                    i32,
                
                    i32,
                    i32,
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                                "from_user_id",
                                "to_user_id",
                            
                                "amount",
                            

                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_transfer() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_transfer() -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "transfer".to_string(),
                mudu_argv_desc_transfer().clone(),
                mudu_result_desc_transfer().clone(),
                false
            )
        })
}

mod mod_transfer {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-transfer;
            world mudu-app-mp2-transfer {
                export mp2-transfer: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestTransfer {}

    impl Guest for GuestTransfer {
        async fn mp2_transfer(param: Vec<u8>) -> Vec<u8> {
            super::mp2_transfer(param).await
        }
    }

    export!(GuestTransfer);
}
async fn mp2_deposit(param: Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_deposit,
    ).await
}

pub async fn mudu_inner_p2_deposit(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_deposit().clone();
    let res = deposit(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
        
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_deposit() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                
                    i32,
                
                    i32,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                                "user_id",
                            
                                "amount",
                            

                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_deposit() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_deposit() -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "deposit".to_string(),
                mudu_argv_desc_deposit().clone(),
                mudu_result_desc_deposit().clone(),
                false
            )
        })
}

mod mod_deposit {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-deposit;
            world mudu-app-mp2-deposit {
                export mp2-deposit: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestDeposit {}

    impl Guest for GuestDeposit {
        async fn mp2_deposit(param: Vec<u8>) -> Vec<u8> {
            super::mp2_deposit(param).await
        }
    }

    export!(GuestDeposit);
}
async fn mp2_update_user(param: Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_update_user,
    ).await
}

pub async fn mudu_inner_p2_update_user(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_update_user().clone();
    let res = update_user(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
        ::mudu_type::datum::value_to_typed::<
            String,
            _,
        >(&param.param_list()[1], "String")?,
        ::mudu_type::datum::value_to_typed::<
            String,
            _,
        >(&param.param_list()[2], "String")?,
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_update_user() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                
                    i32,
                    String,
                    String,
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                            
                                "user_id",
                                "name",
                                "email",
                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_update_user() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_update_user() -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "update_user".to_string(),
                mudu_argv_desc_update_user().clone(),
                mudu_result_desc_update_user().clone(),
                false
            )
        })
}

mod mod_update_user {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-update-user;
            world mudu-app-mp2-update-user {
                export mp2-update-user: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestUpdateUser {}

    impl Guest for GuestUpdateUser {
        async fn mp2_update_user(param: Vec<u8>) -> Vec<u8> {
            super::mp2_update_user(param).await
        }
    }

    export!(GuestUpdateUser);
}
async fn mp2_transfer_funds(param:Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_transfer_funds,
    ).await
}

pub async fn mudu_inner_p2_transfer_funds(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_transfer_funds().clone();
    let res = transfer_funds(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[2], "i32")?,
        
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_transfer_funds()  -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                
                    i32,
                
                    i32,
                
                    i32,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                            
                                "from_user_id",
                            
                                "to_user_id",
                            
                                "amount",
                            

                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_transfer_funds() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_transfer_funds()  -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "transfer_funds".to_string(),
                mudu_argv_desc_transfer_funds().clone(),
                mudu_result_desc_transfer_funds().clone(),
                false
            )
        })
}

mod mod_transfer_funds {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-transfer-funds;
            world mudu-app-mp2-transfer-funds {
                export mp2-transfer-funds: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestTransferFunds {}

    impl Guest for GuestTransferFunds {
        async fn mp2_transfer_funds(param:Vec<u8>) -> Vec<u8> {
            super::mp2_transfer_funds(param).await
        }
    }

    export!(GuestTransferFunds);
}
async fn mp2_delete_user(param: Vec<u8>) -> Vec<u8> {
    ::mudu_binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_delete_user,
    ).await
}

pub async fn mudu_inner_p2_delete_user(
    param: &::mudu_contract::procedure::procedure_param::ProcedureParam,
) -> ::mudu::common::result::RS<
    ::mudu_contract::procedure::procedure_result::ProcedureResult,
> {
    let return_desc = mudu_result_desc_delete_user().clone();
    let res = delete_user(
        param.session_id(),
        
            ::mudu_type::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
        
    ).await;
    let tuple = res;
    Ok(
        ::mudu_contract::procedure::procedure_result::ProcedureResult::from(
            tuple,
            &return_desc,
        )?,
    )
}

pub fn mudu_argv_desc_delete_user() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            <(
                
                    i32,
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum
            >::tuple_desc_static(
                &{
                    let _vec: Vec<String> = <[_]>::into_vec(
                            std::boxed::Box::new([
                            
                                "user_id",
                            

                            ]),
                        )
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    _vec
                },
            )
        }
    )
}

pub fn mudu_result_desc_delete_user() -> &'static ::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mudu_contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            <(
                
            ) as ::mudu_contract::tuple::tuple_datum::TupleDatum>::tuple_desc_static(
                &[],
            )
        }
    )
}

pub fn mudu_proc_desc_delete_user() -> &'static ::mudu_contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mudu_contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mudu_contract::procedure::proc_desc::ProcDesc::new(
                "module".to_string(),
                "delete_user".to_string(),
                mudu_argv_desc_delete_user().clone(),
                mudu_result_desc_delete_user().clone(),
                false
            )
        })
}

mod mod_delete_user {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-delete-user;
            world mudu-app-mp2-delete-user {
                export mp2-delete-user: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestDeleteUser {}

    impl Guest for GuestDeleteUser {
        async fn mp2_delete_user(param: Vec<u8>) -> Vec<u8> {
            super::mp2_delete_user(param).await
        }
    }

    export!(GuestDeleteUser);
}