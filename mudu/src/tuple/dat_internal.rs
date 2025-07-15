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

impl DatInternal {
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

    pub fn to_any<T: 'static + Clone>(&self) -> &T {
        match self {
            Self::Any(v) => v
                .downcast_ref::<T>()
                .unwrap_or_else(|| unsafe { hint::unreachable_unchecked() }),
            // SAFETY: the safety contract must be upheld by the caller.
            Self::Raw(_) => unsafe { hint::unreachable_unchecked() },
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
            Self::Raw(_) => unsafe { hint::unreachable_unchecked() },
        }
    }
}

unsafe impl Send for DatInternal {}
unsafe impl Sync for DatInternal {}
