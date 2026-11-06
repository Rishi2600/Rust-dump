#[proc_macro_derive(SimpleToJson)]
pub fn derive_to_json(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let expanded = quote! {
        impl #name {
            pub fn to_json_string(&self) -> String {
                format!("{{\"struct\": \"{}\"}}", stringify!(#name))
            }
        }
    };
    TokenStream::from(expanded)
}