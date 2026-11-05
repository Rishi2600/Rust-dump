macro_rules! safe_query {
    ($sql:literal, $( $param:expr ),* $(,)?) => {
        {
            let query = format!($sql, $( format!("'{}'", $param) ),*);
            println!("Executing compiled query: {}", query);
            query
        }
    };
}

fn main() {
    let username = "alice";
    let age = 30;

    let query = safe_query!(
        "SELECT * FROM users WHERE username = {} AND age = {}",
        username,
        age
    );

    println!("Final query: {}", query);
}