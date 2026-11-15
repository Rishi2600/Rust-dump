use crate::error::{KResult, KernelError};
use std::cmp::Ordering;

use crate::instr::{AluOp, Cond, NUM_REGS, Operand, Reg};

/// Saved CPU state of one task: what a context switch has to preserve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    pub regs: [i64; NUM_REGS],
    pub pc: usize,
    pub sp: usize,
    pub flags: Ordering,
}

impl Context {
    /// A fresh context starting at instruction 0 with an empty stack.
    pub fn new(stack_top: usize) -> Self {
        Self {
            regs: [0; NUM_REGS],
            pc: 0,
            sp: stack_top,
            flags: Ordering::Equal,
        }
    }

    pub fn get(&self, reg: Reg) -> i64 {
        self.regs[reg.index()]
    }

    pub fn set(&mut self, reg: Reg, value: i64) {
        self.regs[reg.index()] = value;
    }

    pub fn value(&self, operand: Operand) -> i64 {
        match operand {
            Operand::Reg(reg) => self.get(reg),
            Operand::Imm(value) => value,
        }
    }
}

/// Arithmetic/logic unit. Arithmetic wraps; dividing by zero traps.
pub fn alu(op: AluOp, lhs: i64, rhs: i64) -> KResult<i64> {
    Ok(match op {
        AluOp::Add => lhs.wrapping_add(rhs),
        AluOp::Sub => lhs.wrapping_sub(rhs),
        AluOp::Mul => lhs.wrapping_mul(rhs),
        AluOp::Div | AluOp::Mod if rhs == 0 => return Err(KernelError::DivideByZero),
        AluOp::Div => lhs.wrapping_div(rhs),
        AluOp::Mod => lhs.wrapping_rem(rhs),
        AluOp::And => lhs & rhs,
        AluOp::Or => lhs | rhs,
        AluOp::Xor => lhs ^ rhs,
    })
}

impl Cond {
    /// Whether a branch is taken, given the flags left by the last `cmp`.
    pub fn holds(self, flags: Ordering) -> bool {
        match self {
            Cond::Always => true,
            Cond::Eq => flags == Ordering::Equal,
            Cond::Ne => flags != Ordering::Equal,
            Cond::Lt => flags == Ordering::Less,
            Cond::Le => flags != Ordering::Greater,
            Cond::Gt => flags == Ordering::Greater,
            Cond::Ge => flags != Ordering::Less,
        }
    }
}

/// The memory a CPU sees. The kernel plugs in an MMU; tests can use flat RAM.
pub trait Bus {
    fn load(&mut self, addr: usize) -> KResult<i64>;
    fn store(&mut self, addr: usize, value: i64) -> KResult<()>;
}

impl Context {
    /// Push a word onto the stack, which grows towards lower addresses.
    /// `sp` only moves once the store has succeeded, so a fault can be retried.
    pub fn push(&mut self, bus: &mut dyn Bus, value: i64) -> KResult<()> {
        let addr = self.sp.checked_sub(1).ok_or(KernelError::StackOverflow)?;
        bus.store(addr, value)?;
        self.sp = addr;
        Ok(())
    }

    pub fn pop(&mut self, bus: &mut dyn Bus) -> KResult<i64> {
        let value = bus.load(self.sp)?;
        self.sp += 1;
        Ok(value)
    }
}
