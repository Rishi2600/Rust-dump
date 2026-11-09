pub trait SqlRowMapper {
    type Output;
    fn map_row(row: &[u8]) -> Self::Output;
}

struct UserMapper;
impl SqlRowMapper for UserMapper {
    type Output = String;
    fn map_row(_row: &[u8]) -> Self::Output {
        String::from("John Doe")
    }
}