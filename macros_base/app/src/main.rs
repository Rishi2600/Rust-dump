#[proc_macro_attribute]
pub fn get_route(attr: TokenStream, item: TokenStream) -> TokenStream {
    let route_path = attr.to_string();
    let input_fn = parse_macro_input!(item as syn::ItemFn);
    let fn_name = input_fn.sig.ident.clone();

    let expanded = quote! {
        #input_fn
        pub fn register_route() {
            println!("Registering GET route {} to handler {}", #route_path, stringify!(#fn_name));
        }
    };
    TokenStream::from(expanded)
}