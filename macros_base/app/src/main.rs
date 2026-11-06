#[proc_macro]
pub fn embed_env(input: TokenStream) -> TokenStream {
    let var_name = input.to_string().replace('"', "");
    let val = std::env::var(&var_name).unwrap_or_else(|_| "DEFAULT_VAL".to_string());

    let expanded = quote! {
        #val
    };
    TokenStream::from(expanded)
}