use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn mudu_procedure(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // todo do SQL semantic check here
    item
}

