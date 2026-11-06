#[proc_macro]
pub fn compile_fsm(input: TokenStream) -> TokenStream {
    let fsm_def = input.to_string();
    let expanded = quote! {
        pub struct GeneratedStateMachine {
            definition: &'static str,
        }
        impl GeneratedStateMachine {
            pub fn new() -> Self {
                Self { definition: #fsm_def }
            }
        }
    };
    TokenStream::from(expanded)
}