use proc_macro::TokenStream;
use quote::quote;

/*

Suppose there is a procedure function F with the following signature:

``
    fn F(xid:XID, arg1:ty1, arg2: ty2, arg3: ty3, ...) -> RS<(r_ty1, r_ty2, ...)>;
``
    where,

    1. "..." represents possible additional parameters or partial return values.
    2. The function name F can be any valid identifier.
    3. Each parameter in list ty1,ty2,ty3, ... and each return value in tuple r_ty1, r_ty2, ... would
       implement Datum trait

``mudu_macro`` macro expands F into a function named __mudu_macro_F with the following signature:

``
    fn __mudu_macro_F(argv: Vec<Vec<u8>>) -> RS<Vec<Vec<u8>>>;
``

In ``__mudu_macro_F``, implement the following:

    1.Deserialize the argv parameters into the parameter list of F:

        ``arg1:ty1, arg2: ty2, arg3: ty3, ...``

    2. Call F to obtain the return value ret: (r_ty1, r_ty2, ...)

    3. Serialize each value in ret into Vec<u8> and return the result as Vec<Vec<u8>>.
*/

#[proc_macro_attribute]
pub fn mudu_proc(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // todo do SQL semantic check here
    let input_fn = syn::parse_macro_input!(item as syn::ItemFn);

    let fn_ident = &input_fn.sig.ident; // function name
    let fn_generics = &input_fn.sig.generics;
    let fn_input = &input_fn.sig.inputs;
    let fn_output = &input_fn.sig.output;

    let fn_vis = &input_fn.vis;
    // build new function name
    let fn_inner_ident = syn::Ident::new(
        &format!("{}{}", mudu::procedure::proc::MUDU_PROC_INNER_PREFIX, fn_ident), fn_ident.span());
    let fn_wrapper_ident = syn::Ident::new(
        &format!("{}{}", mudu::procedure::proc::MUDU_PROC_PREFIX, fn_ident), fn_ident.span());
    let fn_argv_desc = syn::Ident::new(
        &format!("{}{}", mudu::procedure::proc::MUDU_PROC_ARGV_DESC_PREFIX, fn_ident), fn_ident.span());
    let fn_result_desc = syn::Ident::new(
        &format!("{}{}", mudu::procedure::proc::MUDU_PROC_RESULT_DESC_PREFIX, fn_ident), fn_ident.span());
    match fn_vis {
        syn::Visibility::Public(_) => {}
        _ => {
            panic!("mudu procedure must be public");
        }
    }
    let argc = fn_input.len();
    let check_argv = quote! {
        if argv.len() + 1 != #argc {
            return Err(
                ::mudu::m_error!(
                ::mudu::error::ec::EC::MuduError, format!("expected {} arguments, but found {}", #argc,  argv.len() + 1)));
        }
    };
    let mut fn_invoke_handle_argv = Vec::new();
    let mut tuple_desc_argv = Vec::new();
    for (i, arg) in fn_input.iter().enumerate() {
        let (ts1, ts2) = match arg {
            syn::FnArg::Typed(pat_type) => {
                let pat = &pat_type.pat;
                let ty = &pat_type.ty;

                // type name
                let ty_name = quote!(#ty).to_string();
                let pat_name = quote!(#pat).to_string();
                if i == 0 { // the first argument is transaction id
                    if ty_name != "XID" {
                        panic!("The first argument must be a XID");
                    }
                    (
                        quote! {
                            let #pat: #ty = #pat;
                        },
                        quote! {}
                    )
                } else {
                    (
                        quote! {
                            let #pat: #ty = ::mudu::tuple::datum::binary_to_typed::<#ty, _>(&argv[#i - 1], #ty_name);
                        },
                        quote! {
                            {
                                let (id, _) = mudu::data_type::dt_impl::lang::rust::dt_lang_name_to_id(&#ty_name).unwrap();
                                let #pat : ::mudu::tuple::datum_desc::DatumDesc = ::mudu::tuple::datum_desc::DatumDesc::new(
                                  #pat_name.to_string(),
                                  ::mudu::data_type::dat_type::DatType::new_with_default_param(id)
                                );
                                #pat
                            },
                        }
                    )
                }
            }
            _ => {
                panic!("cannot be self");
            }
        };
        fn_invoke_handle_argv.push(ts1);
        tuple_desc_argv.push(ts2);
    }

    let vec_pat: Vec<&Box<syn::Pat>> = fn_input
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pat_type) => Some(&pat_type.pat),
            _ => None,
        })
        .collect();

    let (handling_return, tuple_desc_result) = if let syn::ReturnType::Type(_, return_type) = fn_output {
        match &**return_type {
            syn::Type::Path(path) => {
                if path.path.segments.len() != 1 {
                    panic!("must be result type")
                } else {
                    let path_segment = &path.path.segments[0];
                    match &path_segment.arguments {
                        syn::PathArguments::AngleBracketed(template_argv) => {
                            if template_argv.args.len() != 1 {
                                panic!("must be 1 argument")
                            }
                            let arg = &template_argv.args[0];
                            match arg {
                                syn::GenericArgument::Type(ty) => {
                                    match ty {
                                        syn::Type::Tuple(tuple) => {
                                            let mut vec_return_binary = vec![];
                                            let mut vec_return_datum_desc = vec![];
                                            for (i, elem_type) in tuple.elems.iter().enumerate() {
                                                let index = syn::Index::from(i);
                                                let type_str = quote!(#elem_type).to_string();
                                                let ts1 = quote! {
                                                        ::mudu::tuple::datum::binary_from_typed(&ret.#index, #type_str)
                                                };
                                                vec_return_binary.push(ts1);

                                                let ts2 = quote! {
                                                    {
                                                        let (id, _) = mudu::data_type::dt_impl::lang::rust::dt_lang_name_to_id(&#type_str).unwrap();
                                                        let name = format!("ret_{}", #index);
                                                        let desc : ::mudu::tuple::datum_desc::DatumDesc = ::mudu::tuple::datum_desc::DatumDesc::new(
                                                          name,
                                                          ::mudu::data_type::dat_type::DatType::new_with_default_param(id)
                                                        );
                                                        desc
                                                    },
                                                };
                                                vec_return_datum_desc.push(ts2);
                                            }
                                            (
                                                vec_return_binary, vec_return_datum_desc
                                            )
                                        }
                                        _ => {
                                            panic!("must be a tuple in Result<> type")
                                        }
                                    }
                                }
                                _ => {
                                    panic!("must be type argument")
                                }
                            }
                        }
                        _ => {
                            panic!("must be angle bracketed")
                        }
                    }
                }
            }
            _ => {
                panic!("must be Result type")
            }
        }
    } else {
        panic!("must be a Result type")
    };

    let expanded = quote! {
        #input_fn

        pub fn #fn_inner_ident #fn_generics (xid:XID, argv: Vec<Vec<u8>>) -> ::mudu::common::result::RS<Vec<Vec<u8>>> {
            (#check_argv);
            #(#fn_invoke_handle_argv)*
            let ret = #fn_ident(#( #vec_pat ),*)?;
            Ok(vec![#(#handling_return),*])
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn #fn_wrapper_ident #fn_generics(p1_ptr: *const u8, p1_len: usize, p2_ptr: *mut u8, p2_len: usize) -> i32 {
            ::mudu::procedure::proc::invoke_proc(
                p1_ptr, p1_len,
                p2_ptr, p2_len,
                #fn_inner_ident #fn_generics
            )
        }

        pub fn #fn_argv_desc #fn_generics() -> &'static ::mudu::tuple::tuple_item_desc::TupleItemDesc {
            static ARGV_DESC: std::sync::OnceLock<::mudu::tuple::tuple_item_desc::TupleItemDesc> =
                std::sync::OnceLock::new();
            ARGV_DESC.get_or_init(||
                {
                    ::mudu::tuple::tuple_item_desc::TupleItemDesc::new(vec![
                        #(#tuple_desc_argv)*
                    ])
                }
            )
        }

        pub fn #fn_result_desc #fn_generics() -> &'static ::mudu::tuple::tuple_item_desc::TupleItemDesc {
            static RESULT_DESC: std::sync::OnceLock<::mudu::tuple::tuple_item_desc::TupleItemDesc> =
                std::sync::OnceLock::new();
            RESULT_DESC.get_or_init(||
                {
                    ::mudu::tuple::tuple_item_desc::TupleItemDesc::new(vec![
                        #(#tuple_desc_result)*
                    ])
                }
            )
        }
    };


    TokenStream::from(expanded)
}





