#[proc_macro]
pub fn i18n_msg(input: TokenStream) -> TokenStream {
    let key = input.to_string();
    let expanded = quote! {
        format!("Localized string for key: {}", #key)
    };
    TokenStream::from(expanded)
}