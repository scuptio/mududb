//! Representation of a discovered Mudu procedure in Rust source code.

use crate::rust::rust_type::RustType;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_type_desc::UniTypeDesc;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;

/// A Rust function annotated with `/**mudu-proc**/`
#[derive(Debug)]
pub struct Function {
    /// Function name.
    pub name: String,
    /// Argument name/type pairs. The first argument is the OID.
    pub arg_list: Vec<(String, RustType)>,
    /// Optional return type.
    pub return_type: Option<RustType>,
    /// Whether the function has been rewritten to async.
    pub is_async: bool,
}

impl Function {
    /// Generate a Mudu [`ProcDesc`] from this function.
    pub fn to_proc_desc(&self, module_name: &str, custom_types: &UniTypeDesc) -> RS<ProcDesc> {
        if self.arg_list.is_empty() {
            return Err(mudu_error!(
                ErrorCode::Internal,
                "procedure must have at least one OID argument"
            ));
        }
        let mut params = Vec::with_capacity(self.arg_list.len() - 1);
        for (name, arg) in self.arg_list[1..].iter() {
            let desc = DatumDesc::new(name.clone(), arg.to_dat_type(custom_types)?);
            params.push(desc);
        }
        let rets = if let Some(ty) = &self.return_type {
            let ret_ty = ty.as_ret_type()?;
            let mut rets = Vec::with_capacity(ret_ty.len());
            for (i, r) in ret_ty.iter().enumerate() {
                let desc = DatumDesc::new(i.to_string(), r.to_dat_type(custom_types)?);
                rets.push(desc);
            }
            rets
        } else {
            vec![]
        };
        Ok(ProcDesc::new(
            module_name.to_owned(),
            self.name.clone(),
            TupleFieldDesc::new(params),
            TupleFieldDesc::new(rets),
            self.is_async,
        ))
    }
}
