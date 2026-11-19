//! Percent-encoding: how a URL carries bytes that would otherwise have a
//! special meaning. A space becomes `%20`, a slash inside a name `%2F`.

use crate::error::{HttpError, HttpResult};

/// Turn every `%XX` back into the byte it stands for.
pub fn percent_decode(input: &str) -> HttpResult<String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let digits = bytes.get(index + 1..index + 3);
        let value = digits
            .filter(|pair| pair.iter().all(u8::is_ascii_hexdigit))
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        let Some(byte) = value else {
            return Err(HttpError::BadRequest(format!(
                "`{input}` has a broken percent escape"
            )));
        };
        decoded.push(byte);
        index += 3;
    }
    String::from_utf8(decoded)
        .map_err(|_| HttpError::BadRequest(format!("`{input}` does not decode to UTF-8")))
}

/// Decode a query string component, where `+` also stands for a space.
pub fn form_decode(input: &str) -> HttpResult<String> {
    percent_decode(&input.replace('+', " "))
}

/// Encode everything except letters, digits and `-._~`.
pub fn percent_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_decode_to_the_bytes_they_stand_for() {
        assert_eq!(percent_decode("hello%20world"), Ok("hello world".into()));
        assert_eq!(percent_decode("a%2Fb%2fc"), Ok("a/b/c".into()));
        assert_eq!(percent_decode("caf%C3%A9"), Ok("café".into()));
        assert_eq!(percent_decode("plain"), Ok("plain".into()));
    }

    #[test]
    fn broken_escapes_are_rejected() {
        for bad in ["100%", "%2", "%zz", "%+1", "%ff"] {
            assert!(
                matches!(percent_decode(bad), Err(HttpError::BadRequest(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn plus_means_space_only_in_form_data() {
        assert_eq!(form_decode("a+b%2Bc"), Ok("a b+c".into()));
        assert_eq!(percent_decode("a+b"), Ok("a+b".into()));
    }

    #[test]
    fn encoding_and_decoding_are_inverses() {
        let original = "my file (final) & co/über.txt";
        let encoded = percent_encode(original);
        assert_eq!(
            encoded,
            "my%20file%20%28final%29%20%26%20co%2F%C3%BCber.txt"
        );
        assert_eq!(percent_decode(&encoded), Ok(original.into()));
    }
}
