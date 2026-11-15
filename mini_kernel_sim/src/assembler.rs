use std::collections::HashMap;

use crate::error::{KResult, KernelError};
use crate::instr::{AluOp, Cond, Instr, Operand, Program, Reg};

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

fn asm_error(line: usize, msg: impl Into<String>) -> KernelError {
    KernelError::Assembly {
        line,
        msg: msg.into(),
    }
}

/// A parsed operand, before labels are resolved to addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Reg(Reg),
    Imm(i64),
    Label(String),
}

pub fn parse_operand(text: &str, line: usize) -> KResult<Token> {
    if let Some(reg) = parse_reg(text) {
        Ok(Token::Reg(reg))
    } else if let Some(value) = parse_imm(text) {
        Ok(Token::Imm(value))
    } else if is_identifier(text) {
        Ok(Token::Label(text.to_string()))
    } else {
        Err(asm_error(line, format!("invalid operand `{text}`")))
    }
}

fn parse_reg(text: &str) -> Option<Reg> {
    let digits = text.strip_prefix(['r', 'R'])?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Reg::new(digits.parse().ok()?)
}

/// Immediates are decimal (`-12`), hexadecimal (`0x1F`) or characters (`'A'`).
fn parse_imm(text: &str) -> Option<i64> {
    let quoted = text
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''));
    if let Some(literal) = quoted {
        let literal = unescape(literal)?;
        let mut chars = literal.chars();
        return match (chars.next(), chars.next()) {
            (Some(ch), None) => Some(ch as i64),
            _ => None,
        };
    }
    match text.strip_prefix("0x") {
        Some(hex) => i64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

/// Expand backslash escapes inside a character or string literal.
fn unescape(text: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        out.push(match chars.next()? {
            'n' => '\n',
            't' => '\t',
            '0' => '\0',
            '\\' => '\\',
            '\'' => '\'',
            '"' => '"',
            _ => return None,
        });
    }
    Some(out)
}

/// Turns one tokenized line into an instruction.
struct Encoder<'a> {
    line: &'a Line,
    mnemonic: &'a str,
    labels: &'a HashMap<String, i64>,
}

impl Encoder<'_> {
    fn error(&self, msg: String) -> KernelError {
        asm_error(self.line.number, msg)
    }

    fn arity(&self, expected: usize) -> KResult<()> {
        let found = self.line.operands.len();
        if found == expected {
            return Ok(());
        }
        let name = self.mnemonic;
        Err(self.error(format!(
            "`{name}` expects {expected} operand(s), found {found}"
        )))
    }

    fn reg(&self, index: usize) -> KResult<Reg> {
        match parse_operand(&self.line.operands[index], self.line.number)? {
            Token::Reg(reg) => Ok(reg),
            _ => {
                let (name, position) = (self.mnemonic, index + 1);
                Err(self.error(format!("operand {position} of `{name}` must be a register")))
            }
        }
    }

    /// A register, an immediate, or a label standing for its address.
    fn value(&self, index: usize) -> KResult<Operand> {
        match parse_operand(&self.line.operands[index], self.line.number)? {
            Token::Reg(reg) => Ok(Operand::Reg(reg)),
            Token::Imm(value) => Ok(Operand::Imm(value)),
            Token::Label(name) => match self.labels.get(&name) {
                Some(address) => Ok(Operand::Imm(*address)),
                None => Err(self.error(format!("undefined label `{name}`"))),
            },
        }
    }

    fn target(&self, index: usize) -> KResult<usize> {
        match self.value(index)? {
            Operand::Imm(address) if address >= 0 => Ok(address as usize),
            _ => Err(self.error("jump target must be a label or address".into())),
        }
    }

    fn nullary(&self, instr: Instr) -> KResult<Instr> {
        self.arity(0)?;
        Ok(instr)
    }

    fn binary(&self, build: impl FnOnce(Reg, Operand) -> Instr) -> KResult<Instr> {
        self.arity(2)?;
        Ok(build(self.reg(0)?, self.value(1)?))
    }

    fn encode(&self) -> KResult<Instr> {
        let name = self.mnemonic;
        if let Some(op) = AluOp::from_mnemonic(name) {
            return self.binary(|dst, src| Instr::Alu(op, dst, src));
        }
        if let Some(cond) = Cond::from_mnemonic(name) {
            self.arity(1)?;
            return Ok(Instr::Jump(cond, self.target(0)?));
        }
        match name {
            "nop" => self.nullary(Instr::Nop),
            "ret" => self.nullary(Instr::Ret),
            "syscall" => self.nullary(Instr::Syscall),
            "halt" => self.nullary(Instr::Halt),
            "mov" => self.binary(Instr::Mov),
            "cmp" => self.binary(Instr::Cmp),
            "load" => self.binary(Instr::Load),
            "store" => self.binary(Instr::Store),
            "push" => {
                self.arity(1)?;
                Ok(Instr::Push(self.value(0)?))
            }
            "pop" => {
                self.arity(1)?;
                Ok(Instr::Pop(self.reg(0)?))
            }
            "call" => {
                self.arity(1)?;
                Ok(Instr::Call(self.target(0)?))
            }
            _ => Err(self.error(format!("unknown mnemonic `{name}`"))),
        }
    }
}

/// Assemble source text into a program, resolving labels to addresses.
pub fn assemble(source: &str) -> KResult<Program> {
    let lines = tokenize(source);
    let mut program = Program::default();
    let mut labels = HashMap::new();

    // Pass 1: bind each label to the index of the instruction that follows it.
    let mut pc = 0;
    for line in &lines {
        if let Some(label) = &line.label {
            if labels.insert(label.clone(), pc as i64).is_some() {
                return Err(asm_error(line.number, format!("duplicate label `{label}`")));
            }
        }
        if line.mnemonic.is_some() {
            pc += 1;
        }
    }

    // Pass 2: encode instructions now that every label is known.
    for line in &lines {
        if let Some(mnemonic) = &line.mnemonic {
            let encoder = Encoder {
                line,
                mnemonic,
                labels: &labels,
            };
            program.instrs.push(encoder.encode()?);
        }
    }
    Ok(program)
}
