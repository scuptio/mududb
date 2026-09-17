//! Language-agnostic analysis of WIT func signatures for MSSP codec generation.
//!
//! The MSSP v1 wire format gives every syscall a request frame whose body is
//! the positional MessagePack array of the WIT-declared arguments and a
//! result frame whose body is `[0u8, value]` / `[1u8, error]`. This module
//! classifies parsed [`WitFuncDef`]s into that shape so the per-language
//! templates only deal with rendering.

use crate::src_gen::wit_def::WitFuncDef;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_data_type::UniDataType;

/// The result shape of a WIT function in MSSP terms.
#[derive(Debug, Clone)]
pub enum FuncResult {
    /// `result<_, E>` — unit success payload encoded as `[0u8, 0u8]`.
    Unit {
        /// The carried error type.
        err: UniDataType,
    },
    /// `result<T, E>` — success payload encoded as `[0u8, T]`.
    Value {
        /// The success payload type.
        ok: UniDataType,
        /// The carried error type.
        err: UniDataType,
    },
}

/// A func signature classified for MSSP codec generation.
#[derive(Debug, Clone)]
pub struct FuncSig {
    /// 1-based ordinal of the function in its interface; this is the pinned
    /// `MessageKind` discriminant.
    pub func_index: u32,
    /// Function name as declared in WIT (kebab-case).
    pub func_name: String,
    /// Doc comments attached to the function.
    pub func_comments: String,
    /// Positional parameters (WIT declaration order).
    pub params: Vec<FuncParam>,
    /// Classified result shape.
    pub result: FuncResult,
}

/// A single positional func parameter.
#[derive(Debug, Clone)]
pub struct FuncParam {
    /// Parameter name as declared in WIT (kebab-case).
    pub param_name: String,
    /// Parameter type.
    pub param_type: UniDataType,
}

/// Classify a parsed [`WitFuncDef`] into the MSSP func shape.
///
/// The function's `func_index` must already be assigned (1-based). The return
/// list must be empty (treated as a unit result) or a single unnamed
/// `result<T, E>` type with a present error arm, matching the MSSP contract.
pub fn analyze_func(func: &WitFuncDef) -> RS<FuncSig> {
    let params = func
        .params
        .iter()
        .map(|field| FuncParam {
            param_name: field.rf_name.clone(),
            param_type: field.rf_type.clone(),
        })
        .collect();
    let result = analyze_returns(func)?;
    Ok(FuncSig {
        func_index: func.func_index,
        func_name: func.func_name.clone(),
        func_comments: func.func_comments.clone(),
        params,
        result,
    })
}

fn analyze_returns(func: &WitFuncDef) -> RS<FuncResult> {
    if func.returns.is_empty() {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "func {} has no result type; MSSP func codecs require result<T, E>",
                func.func_name
            )
        ));
    }
    if func.returns.len() != 1 {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "func {} has {} return values; MSSP func codecs require a single result<T, E>",
                func.func_name,
                func.returns.len()
            )
        ));
    }
    let return_ty = &func.returns[0].rf_type;
    match return_ty {
        UniDataType::Result(result_ty) => {
            let err = result_ty.err.as_ref().map(|ty| (**ty).clone()).ok_or_else(|| {
                mudu_error!(
                    ErrorCode::NotImplemented,
                    format!(
                        "func {} returns result<T> without an error arm; MSSP func codecs require result<T, E>",
                        func.func_name
                    )
                )
            })?;
            match &result_ty.ok {
                Some(ok_ty) => Ok(FuncResult::Value {
                    ok: (**ok_ty).clone(),
                    err,
                }),
                None => Ok(FuncResult::Unit { err }),
            }
        }
        _ => Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "func {} returns {:?}; MSSP func codecs require result<T, E>",
                func.func_name, return_ty
            )
        )),
    }
}

/// Return the error type shared by all functions of a WIT file.
///
/// The MSSP result body is `[0u8, value]` / `[1u8, E]` with a single wire
/// error type `E` per file (for `uni-syscall.wit` this is `uni-error`).
pub fn shared_err_type(sigs: &[FuncSig]) -> RS<UniDataType> {
    let mut shared: Option<UniDataType> = None;
    for sig in sigs {
        let err = match &sig.result {
            FuncResult::Unit { err } => err,
            FuncResult::Value { err, .. } => err,
        };
        match &shared {
            None => shared = Some(err.clone()),
            Some(existing) => {
                if format!("{:?}", existing) != format!("{:?}", err) {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        format!(
                            "func {} uses error type {:?}, expected the shared {:?}",
                            sig.func_name, err, existing
                        )
                    ));
                }
            }
        }
    }
    shared.ok_or_else(|| mudu_error!(ErrorCode::Parse, "no functions to analyze"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{FuncResult, analyze_func, shared_err_type};
    use crate::src_gen::wit_def::WitFuncDef;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_def::RecordField;
    use mudu_binding::universal::uni_result_type::UniResultType;
    use mudu_binding::universal::uni_scalar::UniScalar;

    fn field(name: &str, ty: UniDataType) -> RecordField {
        RecordField::new(String::new(), name.to_string(), ty)
    }

    fn uni_error() -> UniDataType {
        UniDataType::Identifier("uni-error".to_string())
    }

    #[test]
    fn analyze_classifies_value_and_unit_results() -> RS<()> {
        let get = WitFuncDef {
            func_comments: String::new(),
            func_name: "get".to_string(),
            params: vec![
                field("oid", UniDataType::Identifier("uni-oid".to_string())),
                field("key", UniDataType::Binary),
            ],
            returns: vec![field(
                "",
                UniDataType::Result(UniResultType {
                    ok: Some(Box::new(UniDataType::Option(Box::new(UniDataType::Binary)))),
                    err: Some(Box::new(uni_error())),
                }),
            )],
            func_index: 6,
        };
        let sig = analyze_func(&get)?;
        assert_eq!(sig.func_index, 6);
        assert_eq!(sig.params.len(), 2);
        assert!(matches!(sig.result, FuncResult::Value { .. }));

        let close = WitFuncDef {
            func_comments: String::new(),
            func_name: "close-session".to_string(),
            params: vec![],
            returns: vec![field(
                "",
                UniDataType::Result(UniResultType {
                    ok: None,
                    err: Some(Box::new(uni_error())),
                }),
            )],
            func_index: 5,
        };
        let sig = analyze_func(&close)?;
        assert!(matches!(sig.result, FuncResult::Unit { .. }));
        Ok(())
    }

    #[test]
    fn analyze_rejects_non_result_return() {
        let func = WitFuncDef {
            func_comments: String::new(),
            func_name: "plain".to_string(),
            params: vec![],
            returns: vec![field("", UniDataType::Scalar(UniScalar::U32))],
            func_index: 1,
        };
        assert!(analyze_func(&func).is_err());
    }

    #[test]
    fn shared_err_type_requires_uniformity() {
        let make = |err: UniDataType| WitFuncDef {
            func_comments: String::new(),
            func_name: "f".to_string(),
            params: vec![],
            returns: vec![field(
                "",
                UniDataType::Result(UniResultType {
                    ok: None,
                    err: Some(Box::new(err)),
                }),
            )],
            func_index: 1,
        };
        let sigs = vec![analyze_func(&make(uni_error())).unwrap()];
        assert!(shared_err_type(&sigs).is_ok());
        let sigs = vec![
            analyze_func(&make(uni_error())).unwrap(),
            analyze_func(&make(UniDataType::Identifier("other-error".to_string()))).unwrap(),
        ];
        assert!(shared_err_type(&sigs).is_err());
    }
}
