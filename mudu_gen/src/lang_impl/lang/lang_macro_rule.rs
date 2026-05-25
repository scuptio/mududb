#[macro_export]
macro_rules! impl_scalar {
    (
        $lang:ident,
        $((
            $wit_ty:ident,
            $lang_ty_name:expr
        )),+
        $(,)?
    ) => {
        paste!{
            pub fn [<scalar_name_ $lang>](scalar_type:&UniScalar) -> String {
                match scalar_type {
                    $(
                        UniScalar::$wit_ty => {
                            $lang_ty_name.to_string()
                        }
                    )+
                }
            }
        }

    };
}

#[macro_export]
macro_rules! impl_non_scalar {
    (
        $lang:ident,
        $((
            $non_scalar_wit_ty:ident,
            $fn_non_scalar_handle:expr
        )),+
        $(,)?
    ) => {
        paste!{
            pub fn [<non_scalar_name_ $lang>](non_scalar_type:&NonScalarType) -> String {
                match non_scalar_type {
                    $(
                        NonScalarType::$non_scalar_wit_ty(inner) => {
                            $fn_non_scalar_handle(inner)
                        }
                    )+
                }
            }
        }
    };
}

#[macro_export]
macro_rules! impl_lang {
    (
        $((
            $lang_upper:ident,
            $lang_lower:ident
        )),+
        $(,)?
    ) => {
        paste!{

            pub fn lang_scalar_name(lang:&LangKind, scalar_type:&UniScalar) -> String {
                match lang {
                    $(
                        LangKind::$lang_upper => {
                            [<$lang_lower>]::lang_def::[<scalar_name_ $lang_lower>](scalar_type)
                        }
                    )+
                }
            }

            pub fn lang_non_scalar_name(lang:&LangKind, non_scalar_type:&NonScalarType) -> String {
                match lang {
                    $(
                        LangKind::$lang_upper => {
                            [<$lang_lower>]::lang_def::[<non_scalar_name_ $lang_lower>](non_scalar_type)
                        }
                    )+
                }
            }
        }
    };
}
