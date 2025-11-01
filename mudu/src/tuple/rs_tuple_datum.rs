use crate::common::result::RS;
use crate::data_type::dat_type::DatType;
use crate::data_type::dt_impl::lang::rust::dt_lang_name_to_id;
use crate::error::ec::EC;
use crate::m_error;
use crate::tuple::datum::DatumDyn;
use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::enumerable_datum::EnumerableDatum;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use std::any::type_name;

// Defines conversion methods between Rust tuples and binary data with description information.
/**
For a tuple (i32, String)
```
    use crate::mudu::tuple::enumerable_datum::EnumerableDatum;
    use crate::mudu::tuple::rs_tuple_datum::RsTupleDatum;

    let data = (42, "hello".to_string());
    let desc = <(i32, String)>::tuple_desc_static();
    let binary = data.to_binary(desc.fields()).unwrap();
    let decoded = <(i32, String)>::from_binary(&binary, desc.fields()).unwrap();
```
**/

pub trait RsTupleDatum: EnumerableDatum + Sized + 'static {
    fn from_binary(vec_bin: &Vec<Vec<u8>>, desc: &[DatumDesc]) -> RS<Self>;
    fn tuple_desc_static() -> TupleFieldDesc;
}

fn datum_from_binary<T: DatumDyn + Clone + 'static>(slice: &[u8], desc: &DatumDesc) -> RS<T> {
    let internal = desc.dat_type_id().fn_recv()(slice, desc.param_obj())
        .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?;
    let t = internal.into_to_typed::<T>();
    Ok(t)
}

fn datum_to_binary<T: DatumDyn + Clone + 'static>(t: &T, desc: &DatumDesc) -> RS<Vec<u8>> {
    let binary = t.to_binary(desc.param_obj())?;
    Ok(binary.into())
}

fn names_to_desc(names: Vec<String>) -> TupleFieldDesc {
    let desc: Vec<_> = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let (id, _) = dt_lang_name_to_id(name).unwrap();
            let desc = DatumDesc::new(format!("t_{}", i), DatType::new_with_default_param(id));
            desc
        })
        .collect();
    TupleFieldDesc::new(desc)
}

macro_rules! impl_rs_tuple_datum {
    // basic: empty tuple
    () => {
        impl EnumerableDatum for () {
            fn to_binary(&self, _desc: &[DatumDesc]) -> RS<Vec<Vec<u8>>> {
                Ok(vec![])
            }

            fn tuple_desc(&self) -> RS<TupleFieldDesc> {
                Ok(TupleFieldDesc::new(vec![]))
            }
        }

        impl RsTupleDatum for () {
            fn from_binary(_vec_bin: &Vec<Vec<u8>>, _desc: &[DatumDesc]) -> RS<()> {
                Ok(())
            }

            fn tuple_desc_static() -> TupleFieldDesc {
                TupleFieldDesc::new(vec![])
            }
        }
    };

    // recursive：handle tuple (T, T..., T)
    ($($T:ident),+) => {
        impl<$($T: DatumDyn + Clone + 'static),*> EnumerableDatum for ($($T,)*) {
            #[allow(non_snake_case)]
            #[allow(unused_assignments)]
            fn to_binary(&self, desc: &[DatumDesc]) -> RS<Vec<Vec<u8>>> {
                if desc.len() < 1 {
                    panic!("tuple size error");
                }
                let mut vec_binary = Vec::new();
                let ($(ref $T,)*) = *self;
                let mut idx = 0;
                $(
                    vec_binary.push(datum_to_binary($T, &desc[idx])?);
                    idx += 1;
                )*
                Ok(vec_binary)
            }

            fn tuple_desc(&self) -> RS<TupleFieldDesc> {
                Ok(Self::tuple_desc_static())
            }
        }

        impl<$($T: DatumDyn + Clone + 'static),*> RsTupleDatum for ($($T,)*) {
            #[allow(non_snake_case)]
            #[allow(unused_assignments)]
            fn from_binary(vec_bin: &Vec<Vec<u8>>, desc: &[DatumDesc]) -> RS<($($T,)*)> {
                if vec_bin.len() != desc.len() {
                    panic!("tuple size error");
                }
                let mut idx = 0;
                $(
                    let $T = datum_from_binary::<$T>(&vec_bin[idx], &desc[idx])?;
                    idx += 1;
                )*
                Ok(($($T,)*))
            }

            fn tuple_desc_static() -> TupleFieldDesc {
                let names:Vec<String> = vec![
                    $(type_name::<$T>().split("::").last().unwrap_or("").to_string(),)*
                ];
                names_to_desc(names)
            }
        }
    };
}

impl_rs_tuple_datum!();
impl_rs_tuple_datum!(A);
impl_rs_tuple_datum!(A, B);
impl_rs_tuple_datum!(A, B, C);
impl_rs_tuple_datum!(A, B, C, D);
impl_rs_tuple_datum!(A, B, C, D, E);
impl_rs_tuple_datum!(A, B, C, D, E, F);
impl_rs_tuple_datum!(A, B, C, D, E, F, G);
impl_rs_tuple_datum!(A, B, C, D, E, F, G, H);
impl_rs_tuple_datum!(A, B, C, D, E, F, G, H, I);
impl_rs_tuple_datum!(A, B, C, D, E, F, G, H, I, J);
impl_rs_tuple_datum!(A, B, C, D, E, F, G, H, I, J, K);
impl_rs_tuple_datum!(A, B, C, D, E, F, G, H, I, J, K, L);
impl_rs_tuple_datum!(A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_rs_tuple_datum!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_rs_tuple_datum!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
impl_rs_tuple_datum!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);
impl_rs_tuple_datum!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q);
impl_rs_tuple_datum!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R);
impl_rs_tuple_datum!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S);
impl_rs_tuple_datum!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T);
impl_rs_tuple_datum!(
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U
);
impl_rs_tuple_datum!(
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V
);
impl_rs_tuple_datum!(
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W
);
impl_rs_tuple_datum!(
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X
);
impl_rs_tuple_datum!(
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y
);
impl_rs_tuple_datum!(
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z
);
impl_rs_tuple_datum!(
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z, A1
);
impl_rs_tuple_datum!(
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z, A1, B1
);
impl_rs_tuple_datum!(
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z, A1, B1, C1
);
impl_rs_tuple_datum!(
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z, A1, B1, C1, D1
);
impl_rs_tuple_datum!(
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z, A1, B1, C1, D1,
    E1
);
impl_rs_tuple_datum!(
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z, A1, B1, C1, D1,
    E1, F1
);
