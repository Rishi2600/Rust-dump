use mini_kernel_sim::assembler::*;
use mini_kernel_sim::error::KernelError;
use mini_kernel_sim::instr::*;

#[test]
fn tokenize_splits_labels_mnemonics_and_operands() {
    let lines = tokenize("start: MOV r0, 5 ; set up\n\n  # nothing here\nloop:\n  halt");
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].number, 1);
    assert_eq!(lines[0].label.as_deref(), Some("start"));
    assert_eq!(lines[0].mnemonic.as_deref(), Some("mov"));
    assert_eq!(lines[0].operands, ["r0", "5"]);
    assert_eq!(lines[1].number, 4);
    assert_eq!(lines[1].label.as_deref(), Some("loop"));
    assert_eq!(lines[1].mnemonic, None);
    assert_eq!(lines[2].mnemonic.as_deref(), Some("halt"));
    assert!(lines[2].operands.is_empty());
}

#[test]
fn tokenize_keeps_separators_inside_quotes() {
    let source = "msg: .string \"a, b; c: d\" ; it's a comment\nmov r1, ';'\nmov r2, ','";
    let lines = tokenize(source);
    assert_eq!(lines[0].label.as_deref(), Some("msg"));
    assert_eq!(lines[0].operands, ["\"a, b; c: d\""]);
    assert_eq!(lines[1].operands, ["r1", "';'"]);
    assert_eq!(lines[2].operands, ["r2", "','"]);
}

#[test]
fn operands_parse_as_registers_immediates_or_labels() {
    let r7 = Reg::new(7).unwrap();
    assert_eq!(parse_operand("r7", 1), Ok(Token::Reg(r7)));
    assert_eq!(parse_operand("-12", 1), Ok(Token::Imm(-12)));
    assert_eq!(parse_operand("0x1F", 1), Ok(Token::Imm(31)));
    assert_eq!(parse_operand("'A'", 1), Ok(Token::Imm(65)));
    assert_eq!(parse_operand("'\\n'", 1), Ok(Token::Imm(10)));
    assert_eq!(
        parse_operand("loop_1", 1),
        Ok(Token::Label("loop_1".into()))
    );
    // There is no r8, so it reads as a label name.
    assert_eq!(parse_operand("r8", 1), Ok(Token::Label("r8".into())));
    assert!(matches!(
        parse_operand("12abc", 3),
        Err(KernelError::Assembly { line: 3, .. })
    ));
}

#[test]
fn assemble_resolves_forward_and_backward_labels() {
    let source =
        "start: mov r0, 3\nloop: sub r0, 1\n cmp r0, 0\n jgt loop\n jmp done\n nop\ndone: halt";
    let program = assemble(source).unwrap();
    let r0 = Reg::new(0).unwrap();
    assert_eq!(
        program.instrs,
        [
            Instr::Mov(r0, Operand::Imm(3)),
            Instr::Alu(AluOp::Sub, r0, Operand::Imm(1)),
            Instr::Cmp(r0, Operand::Imm(0)),
            Instr::Jump(Cond::Gt, 1),
            Instr::Jump(Cond::Always, 6),
            Instr::Nop,
            Instr::Halt,
        ]
    );
}

#[test]
fn assemble_reports_errors_with_line_numbers() {
    let line_of = |source: &str| match assemble(source) {
        Err(KernelError::Assembly { line, .. }) => line,
        other => panic!("expected an assembly error, got {other:?}"),
    };
    assert_eq!(line_of("nop\nfrob r1"), 2); // unknown mnemonic
    assert_eq!(line_of("mov r1"), 1); // wrong operand count
    assert_eq!(line_of("nop\nnop\njmp nowhere"), 3); // undefined label
    assert_eq!(line_of("a: nop\na: nop"), 2); // duplicate label
    assert_eq!(line_of("mov 5, r1"), 1); // destination must be a register
}
