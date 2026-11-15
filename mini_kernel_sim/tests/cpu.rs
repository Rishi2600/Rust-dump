use mini_kernel_sim::cpu::*;
use mini_kernel_sim::error::KernelError;
use mini_kernel_sim::instr::*;

fn reg(index: u8) -> Reg {
    Reg::new(index).expect("valid register")
}

#[test]
fn context_reads_registers_and_immediates() {
    let mut ctx = Context::new(512);
    ctx.set(reg(2), 99);
    assert_eq!(ctx.get(reg(2)), 99);
    assert_eq!(ctx.value(Operand::Reg(reg(2))), 99);
    assert_eq!(ctx.value(Operand::Imm(-4)), -4);
    assert_eq!((ctx.pc, ctx.sp), (0, 512));
}

#[test]
fn alu_wraps_and_traps_division_by_zero() {
    assert_eq!(alu(AluOp::Add, 2, 3), Ok(5));
    assert_eq!(alu(AluOp::Sub, 2, 3), Ok(-1));
    assert_eq!(alu(AluOp::Mul, i64::MAX, 2), Ok(-2));
    assert_eq!(alu(AluOp::Div, 7, 2), Ok(3));
    assert_eq!(alu(AluOp::Mod, 7, 2), Ok(1));
    assert_eq!(alu(AluOp::Xor, 0b110, 0b011), Ok(0b101));
    assert_eq!(alu(AluOp::Div, 1, 0), Err(KernelError::DivideByZero));
    assert_eq!(alu(AluOp::Mod, 1, 0), Err(KernelError::DivideByZero));
}

#[test]
fn conditions_follow_comparison_flags() {
    use std::cmp::Ordering::{Equal, Greater, Less};
    assert!(Cond::Always.holds(Less));
    assert!(Cond::Eq.holds(Equal) && !Cond::Eq.holds(Less));
    assert!(Cond::Ne.holds(Greater) && !Cond::Ne.holds(Equal));
    assert!(Cond::Lt.holds(Less) && !Cond::Lt.holds(Equal));
    assert!(Cond::Le.holds(Equal) && !Cond::Le.holds(Greater));
    assert!(Cond::Gt.holds(Greater) && !Cond::Gt.holds(Equal));
    assert!(Cond::Ge.holds(Equal) && !Cond::Ge.holds(Less));
}
