pub struct NoTable;
pub struct HasTable;

pub struct QueryBuilder<TableState> {
    query: String,
    _state: TableState,
}

impl QueryBuilder<NoTable> {
    pub fn new() -> Self {
        Self { query: String::from("SELECT *"), _state: NoTable }
    }

    pub fn from(self, table: &str) -> QueryBuilder<HasTable> {
        QueryBuilder {
            query: format!("{} FROM {}", self.query, table),
            _state: HasTable,
        }
    }
}

impl QueryBuilder<HasTable> {
    pub fn build(self) -> String {
        format!("{};", self.query)
    }
}