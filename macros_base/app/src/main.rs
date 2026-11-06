#[proc_macro_derive(TrackMetrics)]
pub fn derive_metrics(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let expanded = quote! {
        impl #name {
            pub fn record_allocation(&self) {
                println!("Metric: Allocated instance of {}", stringify!(#name));
            }
        }
    };
    TokenStream::from(expanded)
}