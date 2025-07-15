use std::fmt;

pub trait SQLStmt: fmt::Debug + fmt::Display + Sync {
    fn to_sql_string(&self) -> String;
}


impl SQLStmt for &str {
    fn to_sql_string(&self) -> String {
        self.to_string()
    }
}

impl SQLStmt for str {
    fn to_sql_string(&self) -> String {
        self.to_string()
    }
}

impl SQLStmt for String {
    fn to_sql_string(&self) -> String {
        self.to_string()
    }
}
