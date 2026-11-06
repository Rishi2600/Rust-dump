#[proc_macro_attribute]
pub fn spawn_background(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as syn::ItemFn);
    let fn_name = input_fn.sig.ident.clone();

    let expanded = quote! {
        #input_fn
        pub fn dispatch_async() {
            std::thread::spawn(|| {
                #fn_name();
            });
        }
    };
    TokenStream::from(expanded)
}