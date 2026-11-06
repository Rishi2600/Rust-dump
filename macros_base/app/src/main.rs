#[proc_macro_attribute]
pub fn transactional(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as syn::ItemFn);
    let vis = input_fn.vis;
    let sig = input_fn.sig;
    let block = input_fn.block;

    let expanded = quote! {
        #vis #sig {
            println!("TX BEGIN");
            let res = (|| { #block })();
            println!("TX COMMIT");
            res
        }
    };
    TokenStream::from(expanded)
}