#[derive(Debug)]
pub struct Header<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

pub fn parse_header<'a>(raw: &'a str) -> Result<Header<'a>, &'static str> {
    let mut parts = raw.splitn(2, ':');
    let key = parts.next().ok_or("Missing header key")?.trim();
    let value = parts.next().ok_or("Missing header value")?.trim();

    if key.is_empty() {
        Err("Key cannot be empty")
    } else {
        Ok(Header { key, value })
    }
}