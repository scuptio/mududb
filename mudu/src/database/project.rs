pub struct Project {
    table_name: &'static str,
    column_name: &'static str,
}

impl Project {
    pub fn new(table_name: &'static str, column_name: &'static str) -> Project {
        Self {
            table_name,
            column_name,
        }
    }

    pub fn table_name(&self) -> &'static str {
        &self.table_name
    }

    pub fn column_name(&self) -> &'static str {
        &self.column_name
    }
}
