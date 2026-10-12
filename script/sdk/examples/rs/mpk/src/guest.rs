//! MP2 ABI bridge using the SDK's canonical procedure codecs.
use super::*;
use mududb::universal::{
    mp_wire::{self, FromValue, ToValue},
    uni_procedure_param::UniProcedureParam,
    uni_procedure_result::UniProcedureResult,
};
use mududb::{UniError, UniResult};
wit_bindgen::generate!({ inline: "package mududb:wallet-rs; world wallet-rs {
export mp2-create-user: async func(param: list<u8>) -> list<u8>;
export mp2-deposit: async func(param: list<u8>) -> list<u8>;
export mp2-withdraw: async func(param: list<u8>) -> list<u8>;
export mp2-transfer-funds: async func(param: list<u8>) -> list<u8>;
export mp2-balance: async func(param: list<u8>) -> list<u8>;
}", async: true });
fn integer(p: &UniProcedureParam, index: usize) -> Result<i64> {
    match p.param_list.get(index) {
        Some(UniDataValue::Scalar(UniScalarValue::I64(v))) => Ok(*v),
        Some(UniDataValue::Scalar(UniScalarValue::I32(v))) => Ok(i64::from(*v)),
        _ => Err("expected integer parameter".into()),
    }
}
fn string(p: &UniProcedureParam, index: usize) -> Result<&str> {
    match p.param_list.get(index) {
        Some(UniDataValue::Scalar(UniScalarValue::String(v))) => Ok(v),
        _ => Err("expected string parameter".into()),
    }
}
async fn invoke(op: u8, input: Vec<u8>) -> Vec<u8> {
    let result: Result<i64> = async {
        let value = mp_wire::decode_value(&input).map_err(|e| e.to_string())?;
        let p = UniProcedureParam::from_value(&value).map_err(|e| e.to_string())?;
        let expected = match op {
            0 | 3 => 3,
            1 | 2 => 2,
            _ => 1,
        };
        if p.param_list.len() != expected {
            return Err("invalid parameter count".into());
        }
        match op {
            0 => create_user(&p.session, integer(&p, 0)?, string(&p, 1)?, string(&p, 2)?).await,
            1 => deposit(&p.session, integer(&p, 0)?, integer(&p, 1)?).await,
            2 => withdraw(&p.session, integer(&p, 0)?, integer(&p, 1)?).await,
            3 => {
                transfer_funds(
                    &p.session,
                    integer(&p, 0)?,
                    integer(&p, 1)?,
                    integer(&p, 2)?,
                )
                .await
            }
            _ => balance(&p.session, integer(&p, 0)?).await,
        }
    }
    .await;
    let result: UniResult<UniProcedureResult, UniError> = match result {
        Ok(value) => UniResult::Ok(UniProcedureResult {
            return_list: vec![i(value)],
        }),
        Err(message) => UniResult::Err(UniError {
            err_code: 50017,
            err_msg: message,
            err_src: "wallet-rs".into(),
            ..Default::default()
        }),
    };
    mp_wire::encode_value(&result.to_value())
}
struct Wallet;
impl Guest for Wallet {
    async fn mp2_create_user(p: Vec<u8>) -> Vec<u8> {
        invoke(0, p).await
    }
    async fn mp2_deposit(p: Vec<u8>) -> Vec<u8> {
        invoke(1, p).await
    }
    async fn mp2_withdraw(p: Vec<u8>) -> Vec<u8> {
        invoke(2, p).await
    }
    async fn mp2_transfer_funds(p: Vec<u8>) -> Vec<u8> {
        invoke(3, p).await
    }
    async fn mp2_balance(p: Vec<u8>) -> Vec<u8> {
        invoke(4, p).await
    }
}
export!(Wallet);
