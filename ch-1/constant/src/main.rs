pub fn execute_middleware<F>(f: F)
where
    F: for<'a> Fn(&'a str) -> &'a str,
{
    let val = "runtime";
    let _res = f(val);
}