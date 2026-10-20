// Representative structural pattern of a procedural macro parser body
pub enum MetaAttribute {
    KeyVal(String, String),
    Flag(String),
}

pub struct CustomMacroInput {
    pub target_ident: String,
    pub attributes: Vec<MetaAttribute>,
}

impl CustomMacroInput {
    pub fn parse_tokens(tokens: &[String]) -> Result<Self, &'static str> {
        let mut iter = tokens.iter();
        let target_ident = iter.next().ok_or("Missing target identifier")?.clone();
        let mut attributes = Vec::new();

        while let Some(tok) = iter.next() {
            if tok.contains('=') {
                let parts: Vec<&str> = tok.split('=').collect();
                attributes.push(MetaAttribute::KeyVal(parts[0].to_string(), parts[1].to_string()));
            } else {
                attributes.push(MetaAttribute::Flag(tok.clone()));
            }
        }

        Ok(Self { target_ident, attributes })
    }
}