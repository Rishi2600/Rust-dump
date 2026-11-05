macro_rules! log_expr {
    ($val:expr) => {{
        let res = $val;
        println!("{} = {:?}", stringify!($val), res);
        res
    }};
}

fn main() {
    let x = log_expr!(42 + 1);
    println!("x = {}", x);
}