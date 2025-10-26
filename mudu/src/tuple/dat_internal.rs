use crate::common::result::RS;
use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::tuple::datum::{Datum, DatumDyn};
use std::any::Any;
use std::hint;
use std::sync::Arc;

#[repr(C)]
#[derive(Copy, Clone)]
pub union DatUnion {
    v_f32: f32,
    v_f64: f64,
    v_i32: i32,
    v_u32: u32,
    v_i64: i64,
    v_u64: u64,
}

#[derive(Clone)]
pub enum DatInternal {
    Raw(DatUnion),
    Any(Arc<dyn Any>),
}

impl DatUnion {
    fn to_typed_ref<T: 'static + Clone>(&self) -> &T {
        if std::any::TypeId::of::<T>() == std::any::TypeId::of::<f32>() {
            unsafe { &*(&self.v_f32 as *const f32 as *const T) }
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<f64>() {
            unsafe { &*(&self.v_f64 as *const f64 as *const T) }
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<i32>() {
            unsafe { &*(&self.v_i32 as *const i32 as *const T) }
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<u32>() {
            unsafe { &*(&self.v_u32 as *const u32 as *const T) }
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<i64>() {
            unsafe { &*(&self.v_i64 as *const i64 as *const T) }
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<u64>() {
            unsafe { &*(&self.v_u64 as *const u64 as *const T) }
        } else {
            panic!("Type not supported by union");
        }
    }
}

impl DatInternal {
    pub fn from_datum<T: Datum + 'static>(datum: T) -> RS<Self> {
        let dat_type_id = T::dat_type_id();
        let ret = if dat_type_id.is_primitive_type() {
            let typed = datum.to_typed(T::datum_desc().param_obj())?;
            match typed {
                DatTyped::I32(t) => Self::from_i32(t),
                DatTyped::I64(t) => Self::from_i64(t),
                DatTyped::F32(t) => Self::from_f32(t),
                DatTyped::F64(t) => Self::from_f64(t),
                _ => {
                    panic!("Type not a primitive type");
                }
            }
        } else {
            Self::from_any_type(datum)
        };

        Ok(ret)
    }
    pub fn from_any_type<T: Any + Clone + 'static>(t: T) -> Self {
        Self::from_any(Arc::new(t))
    }

    fn from_any(v_any: Arc<dyn Any>) -> Self {
        Self::Any(v_any)
    }

    pub fn from_f32(v_f32: f32) -> Self {
        Self::Raw(DatUnion { v_f32 })
    }

    pub fn from_f64(v_f64: f64) -> Self {
        Self::Raw(DatUnion { v_f64 })
    }

    pub fn from_i32(v_i32: i32) -> Self {
        Self::Raw(DatUnion { v_i32 })
    }

    pub fn from_u32(v_u32: u32) -> Self {
        Self::Raw(DatUnion { v_u32 })
    }

    pub fn from_i64(v_i64: i64) -> Self {
        Self::Raw(DatUnion { v_i64 })
    }

    pub fn from_u64(v_u64: u64) -> Self {
        Self::Raw(DatUnion { v_u64 })
    }

    pub fn to_f32(&self) -> f32 {
        unsafe { self.union().v_f32 }
    }

    pub fn to_f64(&self) -> f64 {
        unsafe { self.union().v_f64 }
    }

    pub fn to_i32(&self) -> i32 {
        unsafe { self.union().v_i32 }
    }

    pub fn to_u32(&self) -> u32 {
        unsafe { self.union().v_u32 }
    }

    pub fn to_i64(&self) -> i64 {
        unsafe { self.union().v_i64 }
    }

    pub fn to_u64(&self) -> u64 {
        unsafe { self.union().v_u64 }
    }

    pub fn to_typed_ref<T: 'static + Clone + DatumDyn>(&self) -> &T {
        match self {
            Self::Any(v) => v
                .downcast_ref::<T>()
                .unwrap_or_else(|| unsafe { hint::unreachable_unchecked() })
            ,
            // SAFETY: the safety contract must be upheld by the caller.
            Self::Raw(u) => { u.to_typed_ref::<T>() }
        }
    }

    pub fn into_to_typed<T: 'static + Clone + DatumDyn>(self) -> T {
        match self {
            Self::Any(v) => v
                .downcast_ref::<T>()
                .unwrap_or_else(|| unsafe { hint::unreachable_unchecked() })
                .clone(),
            // SAFETY: the safety contract must be upheld by the caller.
            Self::Raw(u) => u.to_typed_ref::<T>().clone(),
        }
    }

    fn union(&self) -> &DatUnion {
        match self {
            Self::Raw(v) => v,
            // SAFETY: the safety contract must be upheld by the caller.
            Self::Any(_) => unsafe { hint::unreachable_unchecked() },
        }
    }

    pub fn any(&self) -> Arc<dyn Any> {
        match self {
            Self::Any(v) => v.clone(),
            // SAFETY: the safety contract must be upheld by the caller.
            Self::Raw(_u) => unsafe { hint::unreachable_unchecked() },
        }
    }
}

unsafe impl Send for DatInternal {}
unsafe impl Sync for DatInternal {}
