/// One meaningful source line, split into its parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// 1-based line number in the source, for error messages.
    pub number: usize,
    pub label: Option<String>,
    pub mnemonic: Option<String>,
    pub operands: Vec<String>,
}

/// Split source text into statements, dropping comments and blank lines.
///
/// Syntax: `label: mnemonic operand, operand ; comment`. Every part is
/// optional, and `#` also starts a comment.
pub fn tokenize(source: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    for (index, raw) in source.lines().enumerate() {
        let code = match unquoted_positions(raw, &[';', '#']).next() {
            Some(end) => &raw[..end],
            None => raw,
        };
        let (label, statement) = split_label(code.trim());
        let statement = statement.trim();
        if label.is_none() && statement.is_empty() {
            continue;
        }
        let (mnemonic, args) = match statement.split_once(char::is_whitespace) {
            Some((mnemonic, args)) => (mnemonic, args.trim()),
            None => (statement, ""),
        };
        lines.push(Line {
            number: index + 1,
            label,
            mnemonic: (!mnemonic.is_empty()).then(|| mnemonic.to_ascii_lowercase()),
            operands: split_operands(args),
        });
    }
    lines
}

/// Byte offsets of the `separators` that sit outside quoted literals.
fn unquoted_positions<'a>(
    text: &'a str,
    separators: &'a [char],
) -> impl Iterator<Item = usize> + 'a {
    let mut quote = None;
    let mut escaped = false;
    text.char_indices().filter_map(move |(index, ch)| {
        if escaped {
            escaped = false;
        } else if let Some(open) = quote {
            if ch == '\\' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
        } else if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if separators.contains(&ch) {
            return Some(index);
        }
        None
    })
}

fn split_label(code: &str) -> (Option<String>, &str) {
    if let Some((head, rest)) = code.split_once(':') {
        let head = head.trim();
        if is_identifier(head) {
            return (Some(head.to_string()), rest);
        }
    }
    (None, code)
}

fn split_operands(args: &str) -> Vec<String> {
    if args.is_empty() {
        return Vec::new();
    }
    let mut operands = Vec::new();
    let mut start = 0;
    for comma in unquoted_positions(args, &[',']) {
        operands.push(args[start..comma].trim().to_string());
        start = comma + 1;
    }
    operands.push(args[start..].trim().to_string());
    operands
}

pub fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
