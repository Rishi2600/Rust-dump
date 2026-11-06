#[proc_macro_attribute]
pub fn retry_on_failure(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attempts = attr.to_string();
    let input_fn = parse_macro_input!(item as syn::ItemFn);
    let vis = input_fn.vis;
    let sig = input_fn.sig;
    let block = input_fn.block;

    let expanded = quote! {
        #vis #sig {
            let max_retries: usize = #attempts.parse().unwrap_or(3);
            let mut attempt = 0;
            loop {
                attempt += 1;
                println!("Execution attempt {} of {}", attempt, max_retries);
                let res = (|| { #block })();
                if attempt >= max_retries { break res; }
            }
        }
    };
    TokenStream::from(expanded)
}