use mini_kernel_sim::assembler::*;

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
