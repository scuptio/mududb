#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::database::sql_stmt::{AsSQLStmtRef, SQLStmt};

    #[test]
    fn str_impl_to_sql_string() {
        let s: &str = "SELECT 1";
        assert_eq!(s.to_sql_string(), "SELECT 1");
        let cloned: Box<dyn SQLStmt> = s.clone_boxed();
        assert_eq!(cloned.to_sql_string(), "SELECT 1");
    }

    #[test]
    fn string_impl_to_sql_string() {
        let s = "INSERT INTO t VALUES (1)".to_string();
        assert_eq!(s.to_sql_string(), "INSERT INTO t VALUES (1)");
        let cloned: Box<dyn SQLStmt> = s.clone_boxed();
        assert_eq!(cloned.to_sql_string(), "INSERT INTO t VALUES (1)");
    }

    #[test]
    fn as_sql_stmt_ref_for_box() {
        let stmt: Box<dyn SQLStmt> = Box::new("SELECT 1".to_string());
        let r: &dyn SQLStmt = stmt.as_sql_stmt_ref();
        assert_eq!(r.to_sql_string(), "SELECT 1");
    }

    #[test]
    fn as_ref_dyn_sql_stmt_for_string() {
        let s = "SELECT 1".to_string();
        let r: &(dyn SQLStmt + '_) = s.as_ref();
        assert_eq!(r.to_sql_string(), "SELECT 1");
    }

    #[test]
    fn as_ref_dyn_sql_stmt_for_str() {
        let slice: &str = "SELECT 2";
        let r: &(dyn SQLStmt + '_) = (&slice).as_ref();
        assert_eq!(r.to_sql_string(), "SELECT 2");
    }

    #[test]
    fn as_sql_stmt_ref_for_reference() {
        let stmt: Box<dyn SQLStmt> = Box::new("SELECT 1".to_string());
        let r: &dyn SQLStmt = AsSQLStmtRef::as_sql_stmt_ref(&stmt);
        assert_eq!(r.to_sql_string(), "SELECT 1");

        let stmt_ref = &stmt;
        let r2: &dyn SQLStmt = stmt_ref.as_sql_stmt_ref();
        assert_eq!(r2.to_sql_string(), "SELECT 1");
    }

    #[test]
    fn string_slice_clone_boxed() {
        let slice: &str = "SELECT 3";
        let cloned: Box<dyn SQLStmt> = <&str as SQLStmt>::clone_boxed(&slice);
        assert_eq!(cloned.to_sql_string(), "SELECT 3");
    }
}
