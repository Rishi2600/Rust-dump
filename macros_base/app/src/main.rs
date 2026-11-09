use std::sync::OnceLock;

static GLOBAL_CONFIG: OnceLock<Result<String, &'static str>> = OnceLock::new();

pub fn get_config() -> Result<&'static String, &'static str> {
    GLOBAL_CONFIG.get_or_init(|| {
        Ok(String::from("Production Environment"))
    }).as_ref().map_err(|e| *e)
}