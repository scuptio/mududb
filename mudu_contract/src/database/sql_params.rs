use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::result::RS;
use mudu_type::dat_type::DatType;
use mudu_type::datum::DatumDyn;
use paste::paste;


pub trait ToDatum {
    fn to_datum(&self) -> RS<&dyn DatumDyn>;
}


//pub trait DatumDyn: DatumDyn {}


pub trait SQLParams: Send + Sync {
    #[doc(hidden)]
    fn size(&self) -> u64;

    #[doc(hidden)]
    fn get_idx(&self, n: u64) -> Option<&dyn DatumDyn>;

    #[doc(hidden)]
    fn get_idx_unchecked(&self, n: u64) -> &dyn DatumDyn {
        unsafe { self.get_idx(n).unwrap_unchecked() }
    }
    #[doc(hidden)]
    fn param_tuple_desc(&self) -> RS<TupleFieldDesc> {
        let mut vec = Vec::with_capacity(self.size() as usize);
        for i in 0..self.size() {
            let datum = self.get_idx_unchecked(i);
            let dat_type = DatType::default_for(datum.dat_type_id()?);
            let datum_desc = DatumDesc::new(format!("v_{}", i), dat_type);
            vec.push(datum_desc)
        }
        Ok(TupleFieldDesc::new(vec))
    }
    #[doc(hidden)]
    fn param_to_binary(&self, desc: &[DatumDesc]) -> RS<Vec<Vec<u8>>> {
        let size = self.size() as usize;
        if desc.len() != self.size() as usize {
            panic!("desc and vec length do not match");
        }
        let mut vec = Vec::with_capacity(size);
        for i in 0..size {
            let datum_desc = &desc[i];
            let datum = self.get_idx_unchecked(i as u64);
            let binary = datum.to_binary(datum_desc.dat_type())?;
            vec.push(binary.into())
        }
        Ok(vec)
    }
}

/*
impl SQLParams for [&dyn DatumDyn; 0] {
    #[inline]
    fn size(&self) -> u64 {
        0
    }

    #[inline]
    fn get_idx(&self, _n: u64) -> Option<&dyn DatumDyn> {
        None
    }
}


impl SQLParams for [&dyn DatumDyn] {
    #[inline]
    fn size(&self) -> u64 {
        self.len() as u64
    }

    #[inline]
    fn get_idx(&self, n: u64) -> Option<&dyn DatumDyn> {
        if n > self.len() as u64 {
            return None;
        }
        Some(self[n as usize])
    }
}
*/

impl SQLParams for Vec<Box<dyn DatumDyn>> {
    #[inline]
    fn size(&self) -> u64 {
        self.len() as u64
    }

    #[inline]
    fn get_idx(&self, n: u64) -> Option<&dyn DatumDyn> {
        if n > self.len() as u64 {
            return None;
        }
        Some(self[n as usize].as_ref())
    }
}

// Manual impls for the empty and singleton tuple, although the rest are covered
// by macros.

impl SQLParams for () {
    #[inline]
    fn size(&self) -> u64 {
        0
    }

    #[inline]
    fn get_idx(&self, _n: u64) -> Option<&dyn DatumDyn> {
        None
    }
}


impl<T: DatumDyn + SQLParamMarker> SQLParams for T {
    #[inline]
    fn size(&self) -> u64 {
        1
    }

    #[inline]
    fn get_idx(&self, _n: u64) -> Option<&dyn DatumDyn> {
        Some(self)
    }
}

// I'm pretty sure you could tweak the `single_tuple_impl` to accept this.

impl<T: DatumDyn> SQLParams for (T,) {
    #[inline]
    fn size(&self) -> u64 {
        1
    }

    #[inline]
    fn get_idx(&self, _n: u64) -> Option<&dyn DatumDyn> {
        Some(&self.0)
    }
}


pub trait SQLParamMarker {}
macro_rules! impl_sql_param_datum_marker {
    ($(
        $inner_type:ty
    ),+ $(,)?) => {
        $(
            paste! {
                impl SQLParamMarker for $inner_type {

                }
            }
        )+
    };
}

impl_sql_param_datum_marker! {
   i32,
   i64,
   f32,
   f64,
   String
}

// count elements number
macro_rules! count_ids {
    () => (0);
    ($id0:ident $($id:ident)*) => (1 + count_ids!($($id)*));
}

// impl SQLParams trait for tuple
macro_rules! impl_sql_params_for_tuples_indexed {
    ($(($n:tt $type:ident)),*) => {


        impl<$($type: DatumDyn),*> SQLParams for ($($type,)*) {
            #[inline]
            fn size(&self) -> u64 {
                count_ids!($($type)*) as u64
            }

            #[inline]
            fn get_idx(&self, n: u64) -> Option<&dyn DatumDyn> {
                match n {
                    $(
                        $n => Some(&self.$n),
                    )*
                    _ => None,
                }
            }
        }
    };
}

// tuple with various elements
impl_sql_params_for_tuples_indexed!((0 A), (1 B));
impl_sql_params_for_tuples_indexed!((0 A), (1 B), (2 C));
impl_sql_params_for_tuples_indexed!((0 A), (1 B), (2 C), (3 D));
impl_sql_params_for_tuples_indexed!((0 A), (1 B), (2 C), (3 D), (4 E));
impl_sql_params_for_tuples_indexed!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F));
impl_sql_params_for_tuples_indexed!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G));
impl_sql_params_for_tuples_indexed!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H));
impl_sql_params_for_tuples_indexed!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I));
impl_sql_params_for_tuples_indexed!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I), (9 J));
impl_sql_params_for_tuples_indexed!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I), (9 J), (10 K));
impl_sql_params_for_tuples_indexed!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I), (9 J), (10 K), (11 L));
impl_sql_params_for_tuples_indexed!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I), (9 J), (10 K), (11 L), (12 M));
impl_sql_params_for_tuples_indexed!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I), (9 J), (10 K), (11 L), (12 M), (13 N));
impl_sql_params_for_tuples_indexed!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I), (9 J), (10 K), (11 L), (12 M), (13 N), (14 O));
impl_sql_params_for_tuples_indexed!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I), (9 J), (10 K), (11 L), (12 M), (13 N), (14 O), (15 P));
