macro_rules! feature_enabled_code {
    ($feature_name:expr, $code:block) => {
        #[cfg(feature = $feature_name)]
        { $code }
    };
}