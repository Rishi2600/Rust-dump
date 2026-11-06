#[proc_macro_attribute]
pub fn require_role(attr: TokenStream, item: TokenStream) -> TokenStream {
    let role = attr.to_string();
    let input_fn = parse_macro_input!(item as syn::ItemFn);
    let vis = input_fn.vis;
    let sig = input_fn.sig;
    let block = input_fn.block;

    let expanded = quote! {
        #vis #sig {
            println!("Checking security clearance for role: {}", #role);
            #block
        }
    };
    TokenStream::from(expanded)
}