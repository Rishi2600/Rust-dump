macro_rules! json_map {
    ($( $key:expr => $val:expr ),* $(,)?) => {
        {
            let mut map = std::collections::HashMap::new();
            $( map.insert($key.to_string(), $val.to_string()); )*
            map
        }
    };
}