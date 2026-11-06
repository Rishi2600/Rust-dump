#[proc_macro_attribute]
pub fn profile_latency(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as syn::ItemFn);
    let fn_name = input_fn.sig.ident.clone();
    let block = input_fn.block;
    let vis = input_fn.vis;
    let sig = input_fn.sig;

    let expanded = quote! {
        #vis #sig {
            let start = std::time::Instant::now();
            let result = (|| { #block })();
            println!("Function '{}' executed in {:?}", stringify!(#fn_name), start.elapsed());
            result
        }
    };
    TokenStream::from(expanded)
}