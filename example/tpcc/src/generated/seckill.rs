//! Seckill (flash-sale) write-heavy workload procedures.
//!
//! Each `seckill_buy` transaction performs one hot-row UPDATE (stock
//! decrement) plus one order INSERT with a sizable payload, driving the
//! commit/WAL write path instead of read-side machinery. Tables are created
//! client-side (see tpcc_benchmark.rs `init_seckill_schema`), optionally
//! partitioned by `si_id`/`so_item_id` across workers.

use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::contract::{sql_params, sql_stmt};
use mududb::error::ErrorCode;
use mududb::mudu_error;
use mududb::sys_interface::async_api::{mudu_command, mudu_query};

fn require_positive(field: &str, value: i32) -> RS<()> {
    if value <= 0 {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("{field} must be positive, got {value}")
        ));
    }
    Ok(())
}

async fn query_stock(xid: OID, item_id: i32) -> RS<Option<i32>> {
    let mut rs = mudu_query::<i32>(
        xid,
        sql_stmt!(&"SELECT si_stock FROM seckill_item WHERE si_id = ?"),
        sql_params!(&(item_id,)),
    )
    .await?;
    rs.next_record()
}

async fn seckill_seed_inner(xid: OID, item_count: i32, initial_stock: i32) -> RS<()> {
    require_positive("item_count", item_count)?;
    require_positive("initial_stock", initial_stock)?;
    for item_id in 1..=item_count {
        mudu_command(
            xid,
            sql_stmt!(
                &"INSERT INTO seckill_item (si_id, si_name, si_stock, si_sold, si_price) VALUES (?, ?, ?, ?, ?)"
            ),
            sql_params!(&(
                item_id,
                format!("promo-item-{item_id}"),
                initial_stock,
                0,
                100
            )),
        ).await?;
    }
    Ok(())
}

/// Returns "ok" on a successful purchase, "sold_out" when the item has no
/// stock left (the transaction is not aborted; no rows are changed then).
async fn seckill_buy_inner(
    xid: OID,
    item_id: i32,
    order_id: i32,
    user_id: i32,
    amount: i32,
    payload: String,
) -> RS<String> {
    require_positive("item_id", item_id)?;
    require_positive("order_id", order_id)?;
    require_positive("user_id", user_id)?;
    require_positive("amount", amount)?;

    let stock = query_stock(xid, item_id).await?;
    match stock {
        Some(remaining) if remaining > 0 => {
            mudu_command(
                xid,
                sql_stmt!(
                    &"UPDATE seckill_item SET si_stock = si_stock - 1, si_sold = si_sold + 1 WHERE si_id = ?"
                ),
                sql_params!(&(item_id,)),
            ).await?;
            mudu_command(
                xid,
                sql_stmt!(
                    &"INSERT INTO seckill_order (so_item_id, so_id, so_user_id, so_amount, so_payload) VALUES (?, ?, ?, ?, ?)"
                ),
                sql_params!(&(item_id, order_id, user_id, amount, payload)),
            ).await?;
            Ok("ok".to_string())
        }
        _ => Ok("sold_out".to_string()),
    }
}

/**mudu-proc**/
pub async fn seckill_seed(xid: OID, item_count: i32, initial_stock: i32) -> RS<()> {
    seckill_seed_inner(xid, item_count, initial_stock).await
}

/**mudu-proc**/
pub async fn seckill_seed_partitioned(xid: OID, item_count: i32, initial_stock: i32) -> RS<()> {
    seckill_seed_inner(xid, item_count, initial_stock).await
}

/**mudu-proc**/
pub async fn seckill_buy(
    xid: OID,
    item_id: i32,
    order_id: i32,
    user_id: i32,
    amount: i32,
    payload: String,
) -> RS<String> {
    seckill_buy_inner(xid, item_id, order_id, user_id, amount, payload).await
}

/**mudu-proc**/
pub async fn seckill_buy_partitioned(
    xid: OID,
    item_id: i32,
    order_id: i32,
    user_id: i32,
    amount: i32,
    payload: String,
) -> RS<String> {
    seckill_buy_inner(xid, item_id, order_id, user_id, amount, payload).await
}
async fn mp2_seckill_seed_partitioned(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_seckill_seed_partitioned,
    )
    .await
}

pub async fn mudu_inner_p2_seckill_seed_partitioned(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = seckill_seed_partitioned(
        param.session_id(),
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[0], "i32")?,
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[1], "i32")?,
    )
    .await;
    match res {
        Ok(tuple) => {
            let return_list = { vec![] };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_seckill_seed_partitioned()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "item_count".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "initial_stock".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
        ])
    })
}

pub fn mudu_result_desc_seckill_seed_partitioned()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    RESULT_DESC
        .get_or_init(|| ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![]))
}

pub fn mudu_proc_desc_seckill_seed_partitioned()
-> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "tpcc".to_string(),
            "seckill_seed_partitioned".to_string(),
            mudu_argv_desc_seckill_seed_partitioned().clone(),
            mudu_result_desc_seckill_seed_partitioned().clone(),
            false,
        )
    })
}

mod mod_seckill_seed_partitioned {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-seckill-seed-partitioned;
            world mudu-app-mp2-seckill-seed-partitioned {
                export mp2-seckill-seed-partitioned: async func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestSeckillSeedPartitioned {}

    impl Guest for GuestSeckillSeedPartitioned {
        async fn mp2_seckill_seed_partitioned(param: Vec<u8>) -> Vec<u8> {
            super::mp2_seckill_seed_partitioned(param).await
        }
    }

    export!(GuestSeckillSeedPartitioned);
}
async fn mp2_seckill_buy_partitioned(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_seckill_buy_partitioned,
    )
    .await
}

pub async fn mudu_inner_p2_seckill_buy_partitioned(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = seckill_buy_partitioned(
        param.session_id(),
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[0], "i32")?,
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[1], "i32")?,
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[2], "i32")?,
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[3], "i32")?,
        ::mududb::types::datum::value_to_typed::<String, _>(&param.param_list()[4], "String")?,
    )
    .await;
    match res {
        Ok(tuple) => {
            let return_list = { vec![::mududb::types::datum::value_from_typed(&tuple, "String")?] };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_seckill_buy_partitioned()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "item_id".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "order_id".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "user_id".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "amount".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "payload".to_string(),
                <String as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
        ])
    })
}

pub fn mudu_result_desc_seckill_buy_partitioned()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "0".to_string(),
                <String as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
        ])
    })
}

pub fn mudu_proc_desc_seckill_buy_partitioned()
-> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "tpcc".to_string(),
            "seckill_buy_partitioned".to_string(),
            mudu_argv_desc_seckill_buy_partitioned().clone(),
            mudu_result_desc_seckill_buy_partitioned().clone(),
            false,
        )
    })
}

mod mod_seckill_buy_partitioned {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-seckill-buy-partitioned;
            world mudu-app-mp2-seckill-buy-partitioned {
                export mp2-seckill-buy-partitioned: async func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestSeckillBuyPartitioned {}

    impl Guest for GuestSeckillBuyPartitioned {
        async fn mp2_seckill_buy_partitioned(param: Vec<u8>) -> Vec<u8> {
            super::mp2_seckill_buy_partitioned(param).await
        }
    }

    export!(GuestSeckillBuyPartitioned);
}
async fn mp2_seckill_seed(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_seckill_seed,
    )
    .await
}

pub async fn mudu_inner_p2_seckill_seed(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = seckill_seed(
        param.session_id(),
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[0], "i32")?,
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[1], "i32")?,
    )
    .await;
    match res {
        Ok(tuple) => {
            let return_list = { vec![] };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_seckill_seed()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "item_count".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "initial_stock".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
        ])
    })
}

pub fn mudu_result_desc_seckill_seed()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    RESULT_DESC
        .get_or_init(|| ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![]))
}

pub fn mudu_proc_desc_seckill_seed() -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc
{
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "tpcc".to_string(),
            "seckill_seed".to_string(),
            mudu_argv_desc_seckill_seed().clone(),
            mudu_result_desc_seckill_seed().clone(),
            false,
        )
    })
}

mod mod_seckill_seed {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-seckill-seed;
            world mudu-app-mp2-seckill-seed {
                export mp2-seckill-seed: async func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestSeckillSeed {}

    impl Guest for GuestSeckillSeed {
        async fn mp2_seckill_seed(param: Vec<u8>) -> Vec<u8> {
            super::mp2_seckill_seed(param).await
        }
    }

    export!(GuestSeckillSeed);
}
async fn mp2_seckill_buy(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_seckill_buy,
    )
    .await
}

pub async fn mudu_inner_p2_seckill_buy(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<::mududb::contract::procedure::procedure_result::ProcedureResult>
{
    let res = seckill_buy(
        param.session_id(),
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[0], "i32")?,
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[1], "i32")?,
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[2], "i32")?,
        ::mududb::types::datum::value_to_typed::<i32, _>(&param.param_list()[3], "i32")?,
        ::mududb::types::datum::value_to_typed::<String, _>(&param.param_list()[4], "String")?,
    )
    .await;
    match res {
        Ok(tuple) => {
            let return_list = { vec![::mududb::types::datum::value_from_typed(&tuple, "String")?] };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_seckill_buy()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "item_id".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "order_id".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "user_id".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "amount".to_string(),
                <i32 as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "payload".to_string(),
                <String as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
        ])
    })
}

pub fn mudu_result_desc_seckill_buy()
-> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    > = std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(|| {
        ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
            ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                "0".to_string(),
                <String as ::mududb::types::datum::Datum>::data_type().clone(),
            ),
        ])
    })
}

pub fn mudu_proc_desc_seckill_buy() -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<::mududb::contract::procedure::proc_desc::ProcDesc> =
        std::sync::OnceLock::new();
    _PROC_DESC.get_or_init(|| {
        ::mududb::contract::procedure::proc_desc::ProcDesc::new(
            "tpcc".to_string(),
            "seckill_buy".to_string(),
            mudu_argv_desc_seckill_buy().clone(),
            mudu_result_desc_seckill_buy().clone(),
            false,
        )
    })
}

mod mod_seckill_buy {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-seckill-buy;
            world mudu-app-mp2-seckill-buy {
                export mp2-seckill-buy: async func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestSeckillBuy {}

    impl Guest for GuestSeckillBuy {
        async fn mp2_seckill_buy(param: Vec<u8>) -> Vec<u8> {
            super::mp2_seckill_buy(param).await
        }
    }

    export!(GuestSeckillBuy);
}
