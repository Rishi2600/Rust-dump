#[proc_macro_derive(BuilderPattern)]
pub fn derive_builder(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let builder_name = syn::Ident::new(&format!("{}Builder", name), name.span());

    let expanded = quote! {
        pub struct #builder_name {
            pub active: bool,
        }
        impl #builder_name {
            pub fn new() -> Self { Self { active: true } }
        }
    };
    TokenStream::from(expanded)
}