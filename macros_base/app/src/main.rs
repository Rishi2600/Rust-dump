#[proc_macro_derive(Introspectable)]
pub fn derive_introspectable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let expanded = quote! {
        impl #name {
            pub fn type_name() -> &'static str {
                stringify!(#name)
            }
        }
    };
    TokenStream::from(expanded)
}