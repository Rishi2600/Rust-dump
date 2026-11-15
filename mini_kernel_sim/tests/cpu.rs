use mini_kernel_sim::cpu::*;
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
