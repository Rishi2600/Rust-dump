#[proc_macro_derive(VariantName)]
pub fn derive_variant_name(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let expanded = quote! {
        impl #name {
            pub fn variant_label(&self) -> &'static str {
                stringify!(#name)
            }
        }
    };
    TokenStream::from(expanded)
}