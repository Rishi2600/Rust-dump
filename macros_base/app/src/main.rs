// In macro crate: proc-macro = true
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(Accessors)]
pub fn derive_accessors(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let expanded = quote! {
        impl #name {
            pub fn inspect(&self) {
                println!("Inspecting struct: {}", stringify!(#name));
            }
        }
    };
    TokenStream::from(expanded)
}