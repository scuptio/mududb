use crate::rust::rust_type::RustType;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_binding::universal::uni_type_desc::UniTypeDesc;

#[derive(Debug)]
pub struct Function {
    pub name: String,
    pub arg_list: Vec<(String, RustType)>,
    pub return_type: Option<RustType>,
    pub is_async: bool,
}

impl Function {
    pub fn to_proc_desc(&self, module_name: &String, custom_types: &UniTypeDesc) -> RS<ProcDesc> {
        if self.arg_list.len() < 1 {
            return Err(m_error!(EC::InternalErr, "procedure must have at least one OID argument"));
        }
        let mut params = Vec::with_capacity(self.arg_list.len() - 1);
        for (name, arg) in self.arg_list[1..].iter() {
            let desc = DatumDesc::new(name.clone(), arg.to_dat_type(custom_types)?);
            params.push(desc);
        }
        let rets = if let Some(ty) = &self.return_type {
            let ret_ty = ty.as_ret_type();
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
            module_name.clone(),
            self.name.clone(),
            TupleFieldDesc::new(params),
            TupleFieldDesc::new(rets),
            self.is_async,
        ))
    }
}