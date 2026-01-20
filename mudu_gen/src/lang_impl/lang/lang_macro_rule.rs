
#[macro_export]
macro_rules! impl_primitive {
    (
        $lang:ident,
        $((
            $wit_ty:ident,
            $lang_ty_name:expr
        )),+
        $(,)?
    ) => {
        paste!{
            pub fn [<primitive_name_ $lang>](primitive_type:&UniPrimitive) -> String {
                match primitive_type {
                    $(
                        UniPrimitive::$wit_ty => {
                            $lang_ty_name.to_string()
                        }
                    )+
                }
            }
        }

    };
}



#[macro_export]
macro_rules! impl_non_primitive {
    (
        $lang:ident,
        $((
            $non_prim_wit_ty:ident,
            $fn_non_prim_handle:expr
        )),+
        $(,)?
    ) => {
        paste!{
            pub fn [<non_primitive_name_ $lang>](non_prim_type:&NonPrimitiveType) -> String {
                match non_prim_type {
                    $(
                        NonPrimitiveType::$non_prim_wit_ty(inner) => {
                            $fn_non_prim_handle(inner)
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

            pub fn lang_primitive_name(lang:&LangKind, primitive_type:&UniPrimitive) -> String {
                match lang {
                    $(
                        LangKind::$lang_upper => {
                            [<$lang_lower>]::lang_def::[<primitive_name_ $lang_lower>](primitive_type)
                        }
                    )+
                }
            }

            pub fn lang_non_primitive_name(lang:&LangKind, non_primitive_type:&NonPrimitiveType) -> String {
                match lang {
                    $(
                        LangKind::$lang_upper => {
                            [<$lang_lower>]::lang_def::[<non_primitive_name_ $lang_lower>](non_primitive_type)
                        }
                    )+
                }
            }
        }
    };
}