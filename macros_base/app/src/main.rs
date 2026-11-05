macro_rules! define_app_errors {
    ($($err_name:ident),* $(,)?) => {
        #[derive(Debug)]
        pub enum AppError {
            $( $err_name(String), )*
        }
    };
}