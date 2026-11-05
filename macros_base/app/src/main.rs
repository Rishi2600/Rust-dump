macro_rules! hygienic_var {
    () => {
        let x = 42; // 'x' is hygienic and isolated to macro expansion scope
        println!("Hygienic x: {}", x);
    };
}