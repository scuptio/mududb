use crate::common::result::RS;
use crate::data_type::dat_type::DatType;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::data_type::dt_impl::lang::rust::dt_lang_name_to_id;
use crate::data_type::param_obj::ParamObj;
use crate::error::ec::EC;
use crate::m_error;
use crate::tuple::dat_binary::DatBinary;
use crate::tuple::dat_internal::DatInternal;
use crate::tuple::dat_printable::DatPrintable;
use crate::tuple::datum_desc::DatumDesc;
use lazy_static::lazy_static;
use std::any::Any;
use std::fmt;

pub trait Datum: DatumDyn + Clone + 'static {
    fn dat_type_id() -> DatTypeID;

    fn datum_desc() -> &'static DatumDesc;
}

pub trait DatumDyn: fmt::Debug + Sync + Any {
    fn dat_type_id_self(&self) -> RS<DatTypeID>;

    fn to_typed(&self, param: &ParamObj) -> RS<DatTyped>;

    fn to_binary(&self, param: &ParamObj) -> RS<DatBinary>;

    fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable>;

    fn to_internal(&self, param: &ParamObj) -> RS<DatInternal>;

    fn clone_boxed(&self) -> Box<dyn DatumDyn>;
}

pub trait AsDatumDynRef {
    fn as_datum_dyn_ref(&self) -> &dyn DatumDyn;
}

impl AsDatumDynRef for Box<dyn DatumDyn> {
    fn as_datum_dyn_ref(&self) -> &dyn DatumDyn {
        self.as_ref()
    }
}

impl<U: AsDatumDynRef + ?Sized> AsDatumDynRef for &U {
    fn as_datum_dyn_ref(&self) -> &dyn DatumDyn {
        (*self).as_datum_dyn_ref()
    }
}

impl<'a, U: AsDatumDynRef> AsDatumDynRef for &'a [U] {
    fn as_datum_dyn_ref(&self) -> &dyn DatumDyn {
        if self.is_empty() {
            panic!("Empty slice");
        }
        self[0].as_datum_dyn_ref()
    }
}

impl<T: AsDatumDynRef> AsDatumDynRef for Vec<T> {
    fn as_datum_dyn_ref(&self) -> &dyn DatumDyn {
        if self.is_empty() {
            panic!("Empty vector");
        }
        self[0].as_datum_dyn_ref()
    }
}

impl<T: AsDatumDynRef, const N: usize> AsDatumDynRef for [T; N] {
    fn as_datum_dyn_ref(&self) -> &dyn DatumDyn {
        if self.is_empty() {
            panic!("Empty array");
        }
        self[0].as_datum_dyn_ref()
    }
}

impl Datum for i32 {
    fn dat_type_id() -> DatTypeID {
        DatTypeID::I32
    }

    fn datum_desc() -> &'static DatumDesc {
        lazy_static! {
            static ref DESC: DatumDesc = DatumDesc::new(
                "".to_string(),
                DatType::new_with_default_param(i32::dat_type_id())
            );
        }
        &DESC
    }
}

impl Datum for i64 {
    fn dat_type_id() -> DatTypeID {
        DatTypeID::I64
    }

    fn datum_desc() -> &'static DatumDesc {
        lazy_static! {
            static ref DESC: DatumDesc = DatumDesc::new(
                "".to_string(),
                DatType::new_with_default_param(i64::dat_type_id())
            );
        }
        &DESC
    }
}

impl Datum for f32 {
    fn dat_type_id() -> DatTypeID {
        DatTypeID::F32
    }

    fn datum_desc() -> &'static DatumDesc {
        lazy_static! {
            static ref DESC: DatumDesc = DatumDesc::new(
                "".to_string(),
                DatType::new_with_default_param(f32::dat_type_id())
            );
        }
        &DESC
    }
}

impl Datum for f64 {
    fn dat_type_id() -> DatTypeID {
        DatTypeID::F64
    }

    fn datum_desc() -> &'static DatumDesc {
        lazy_static! {
            static ref DESC: DatumDesc = DatumDesc::new(
                "".to_string(),
                DatType::new_with_default_param(f64::dat_type_id())
            );
        }
        &DESC
    }
}

impl Datum for String {
    fn dat_type_id() -> DatTypeID {
        DatTypeID::CharVarLen
    }

    fn datum_desc() -> &'static DatumDesc {
        lazy_static! {
            static ref DESC: DatumDesc = DatumDesc::new(
                "".to_string(),
                DatType::new_with_default_param(String::dat_type_id())
            );
        }
        &DESC
    }
}

impl DatumDyn for i32 {
    fn dat_type_id_self(&self) -> RS<DatTypeID> {
        Ok(DatTypeID::I32)
    }

    fn to_typed(&self, param: &ParamObj) -> RS<DatTyped> {
        if param.dat_type_id() != DatTypeID::I32 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(DatTyped::I32(self.clone()))
    }

    fn to_binary(&self, param: &ParamObj) -> RS<DatBinary> {
        if param.dat_type_id() != DatTypeID::I32 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(
            (param.dat_type_id().fn_base().send)(&DatInternal::from_i32(*self), param)
                .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?,
        )
    }

    fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable> {
        if param.dat_type_id() != DatTypeID::I32 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(
            (param.dat_type_id().fn_base().output)(&DatInternal::from_i32(*self), param)
                .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?,
        )
    }

    fn to_internal(&self, param: &ParamObj) -> RS<DatInternal> {
        if param.dat_type_id() != DatTypeID::I32 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(DatInternal::from_i32(*self))
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        Box::new(self.clone())
    }
}

impl DatumDyn for i64 {
    fn dat_type_id_self(&self) -> RS<DatTypeID> {
        Ok(DatTypeID::I64)
    }

    fn to_typed(&self, param: &ParamObj) -> RS<DatTyped> {
        if param.dat_type_id() != DatTypeID::I64 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(DatTyped::I64(self.clone()))
    }

    fn to_binary(&self, param: &ParamObj) -> RS<DatBinary> {
        if param.dat_type_id() != DatTypeID::I64 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(
            (param.dat_type_id().fn_base().send)(&DatInternal::from_i64(*self), param)
                .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?,
        )
    }

    fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable> {
        if param.dat_type_id() != DatTypeID::I64 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(
            (param.dat_type_id().fn_base().output)(&DatInternal::from_i64(*self), param)
                .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?,
        )
    }

    fn to_internal(&self, _: &ParamObj) -> RS<DatInternal> {
        Ok(DatInternal::from_i64(*self))
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        Box::new(self.clone())
    }
}

impl DatumDyn for f32 {
    fn dat_type_id_self(&self) -> RS<DatTypeID> {
        Ok(DatTypeID::F32)
    }

    fn to_typed(&self, param: &ParamObj) -> RS<DatTyped> {
        if param.dat_type_id() != DatTypeID::F32 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(DatTyped::F32(self.clone()))
    }

    fn to_binary(&self, param: &ParamObj) -> RS<DatBinary> {
        if param.dat_type_id() != DatTypeID::F32 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(
            (param.dat_type_id().fn_base().send)(&DatInternal::from_f32(*self), param)
                .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?,
        )
    }

    fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable> {
        if param.dat_type_id() != DatTypeID::F32 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(
            (param.dat_type_id().fn_base().output)(&DatInternal::from_f32(*self), param)
                .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?,
        )
    }

    fn to_internal(&self, param: &ParamObj) -> RS<DatInternal> {
        if param.dat_type_id() != DatTypeID::F32 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(DatInternal::from_f32(*self))
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        Box::new(self.clone())
    }
}

impl DatumDyn for f64 {
    fn dat_type_id_self(&self) -> RS<DatTypeID> {
        Ok(DatTypeID::F64)
    }

    fn to_typed(&self, param: &ParamObj) -> RS<DatTyped> {
        if param.dat_type_id() != DatTypeID::F64 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(DatTyped::F64(self.clone()))
    }

    fn to_binary(&self, param: &ParamObj) -> RS<DatBinary> {
        if param.dat_type_id() != DatTypeID::F64 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(
            (param.dat_type_id().fn_base().send)(&DatInternal::from_f64(*self), param)
                .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?,
        )
    }

    fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable> {
        if param.dat_type_id() != DatTypeID::F64 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(
            (param.dat_type_id().fn_base().output)(&DatInternal::from_f64(*self), param)
                .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?,
        )
    }

    fn to_internal(&self, param: &ParamObj) -> RS<DatInternal> {
        if param.dat_type_id() != DatTypeID::F64 {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(DatInternal::from_f64(*self))
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        Box::new(self.clone())
    }
}

impl DatumDyn for String {
    fn dat_type_id_self(&self) -> RS<DatTypeID> {
        Ok(DatTypeID::CharVarLen)
    }

    fn to_typed(&self, param: &ParamObj) -> RS<DatTyped> {
        if param.dat_type_id() != DatTypeID::CharVarLen
            && param.dat_type_id() != DatTypeID::CharFixedLen
        {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(DatTyped::String(self.clone()))
    }

    fn to_binary(&self, param: &ParamObj) -> RS<DatBinary> {
        if param.dat_type_id() != DatTypeID::CharFixedLen
            && param.dat_type_id() != DatTypeID::CharVarLen
        {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(
            (param.dat_type_id().fn_base().send)(&DatInternal::from_any_type(self.clone()), param)
                .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?,
        )
    }

    fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable> {
        if param.dat_type_id() != DatTypeID::CharVarLen
            && param.dat_type_id() != DatTypeID::CharFixedLen
        {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(
            (param.dat_type_id().fn_base().output)(
                &DatInternal::from_any_type(self.clone()),
                param,
            )
            .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?,
        )
    }

    fn to_internal(&self, param: &ParamObj) -> RS<DatInternal> {
        if param.dat_type_id() != DatTypeID::CharVarLen
            && param.dat_type_id() != DatTypeID::CharFixedLen
        {
            return Err(m_error!(EC::TypeErr));
        }
        Ok(DatInternal::from_any_type(self.clone()))
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        Box::new(self.clone())
    }
}

pub fn binary_to_typed<T: 'static + Clone + DatumDyn, S: AsRef<str>>(
    data: &[u8],
    type_str: S,
) -> T {
    let (id, _) = dt_lang_name_to_id(type_str.as_ref()).unwrap();
    let param = ParamObj::default_for(id);
    let internal = id.fn_recv()(data, &param).unwrap();
    let t = internal.to_typed_ref::<T>();
    t.clone()
}

pub fn binary_from_typed<T: 'static + DatumDyn + Clone, S: AsRef<str>>(
    t: &T,
    type_str: S,
) -> Vec<u8> {
    let (id, _) = dt_lang_name_to_id(type_str.as_ref()).unwrap();
    let param = ParamObj::default_for(id);
    let binary = t.to_binary(&param).unwrap();
    binary.into()
}
