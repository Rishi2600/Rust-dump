use std::fmt;

pub const NUM_REGS: usize = 8;

/// A general-purpose register, `r0` through `r7`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reg(u8);

impl Reg {
    pub fn new(index: u8) -> Option<Self> {
        (usize::from(index) < NUM_REGS).then_some(Self(index))
    }

    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

/// Source operand: either a register or an immediate value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    Reg(Reg),
    Imm(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AluOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Xor,
}

impl AluOp {
    pub const ALL: [AluOp; 8] = [
        AluOp::Add,
        AluOp::Sub,
        AluOp::Mul,
        AluOp::Div,
        AluOp::Mod,
        AluOp::And,
        AluOp::Or,
        AluOp::Xor,
    ];

    pub fn mnemonic(self) -> &'static str {
        match self {
            AluOp::Add => "add",
            AluOp::Sub => "sub",
            AluOp::Mul => "mul",
            AluOp::Div => "div",
            AluOp::Mod => "mod",
            AluOp::And => "and",
            AluOp::Or => "or",
            AluOp::Xor => "xor",
        }
    }

    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|op| op.mnemonic() == text)
    }
}

/// Branch condition, evaluated against the flags left by the last `cmp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cond {
    Always,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Cond {
    pub const ALL: [Cond; 7] = [
        Cond::Always,
        Cond::Eq,
        Cond::Ne,
        Cond::Lt,
        Cond::Le,
        Cond::Gt,
        Cond::Ge,
    ];

    pub fn mnemonic(self) -> &'static str {
        match self {
            Cond::Always => "jmp",
            Cond::Eq => "jeq",
            Cond::Ne => "jne",
            Cond::Lt => "jlt",
            Cond::Le => "jle",
            Cond::Gt => "jgt",
            Cond::Ge => "jge",
        }
    }

    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|cond| cond.mnemonic() == text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instr {
    Nop,
    /// `mov dst, src`
    Mov(Reg, Operand),
    /// `add dst, src` and friends: `dst = dst op src`
    Alu(AluOp, Reg, Operand),
    /// `cmp lhs, rhs`: set the flags for a following conditional jump
    Cmp(Reg, Operand),
    /// Jump to an instruction index when the condition holds
    Jump(Cond, usize),
    /// `load dst, addr`: read a word of memory
    Load(Reg, Operand),
    /// `store src, addr`: write a word of memory
    Store(Reg, Operand),
    Push(Operand),
    Pop(Reg),
    Call(usize),
    Ret,
    /// Trap into the kernel
    Syscall,
    Halt,
}

/// An assembled program: code plus the initial contents of its data segment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Program {
    pub instrs: Vec<Instr>,
    pub data: Vec<i64>,
}

impl fmt::Display for Reg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "r{}", self.0)
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Reg(reg) => write!(f, "{reg}"),
            Operand::Imm(value) => write!(f, "{value}"),
        }
    }
}

/// Disassembly: prints an instruction in the syntax the assembler accepts.
impl fmt::Display for Instr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Instr::Nop => write!(f, "nop"),
            Instr::Mov(dst, src) => write!(f, "mov {dst}, {src}"),
            Instr::Alu(op, dst, src) => write!(f, "{} {dst}, {src}", op.mnemonic()),
            Instr::Cmp(lhs, rhs) => write!(f, "cmp {lhs}, {rhs}"),
            Instr::Jump(cond, target) => write!(f, "{} {target}", cond.mnemonic()),
            Instr::Load(dst, addr) => write!(f, "load {dst}, {addr}"),
            Instr::Store(src, addr) => write!(f, "store {src}, {addr}"),
            Instr::Push(src) => write!(f, "push {src}"),
            Instr::Pop(dst) => write!(f, "pop {dst}"),
            Instr::Call(target) => write!(f, "call {target}"),
            Instr::Ret => write!(f, "ret"),
            Instr::Syscall => write!(f, "syscall"),
            Instr::Halt => write!(f, "halt"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mnemonics_round_trip() {
        for op in AluOp::ALL {
            assert_eq!(AluOp::from_mnemonic(op.mnemonic()), Some(op));
        }
        for cond in Cond::ALL {
            assert_eq!(Cond::from_mnemonic(cond.mnemonic()), Some(cond));
        }
        assert_eq!(AluOp::from_mnemonic("jmp"), None);
    }

    #[test]
    fn register_indices_are_bounded() {
        assert_eq!(Reg::new(7).map(Reg::index), Some(7));
        assert_eq!(Reg::new(8), None);
    }
}
