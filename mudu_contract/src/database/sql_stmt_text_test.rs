#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::database::sql_stmt::SQLStmt;
    use crate::database::sql_stmt_text::SQLStmtText;

    #[test]
    fn sql_stmt_text_new_and_into() {
        let text = SQLStmtText::new("SELECT 1".to_string());
        assert_eq!(text.into(), "SELECT 1");
    }

    #[test]
    fn sql_stmt_text_param_ty_is_empty() {
        let text = SQLStmtText::new("SELECT ?".to_string());
        assert!(text.param_ty().is_empty());
    }

    #[test]
    fn sql_stmt_text_to_sql_string() {
        let text = SQLStmtText::new("INSERT INTO t VALUES (1)".to_string());
        assert_eq!(text.to_sql_string(), "INSERT INTO t VALUES (1)");
    }

    #[test]
    fn sql_stmt_text_clone_boxed() {
        let text = SQLStmtText::new("SELECT 1".to_string());
        let cloned = text.clone_boxed();
        assert_eq!(cloned.to_sql_string(), "SELECT 1");
    }

    #[test]
    fn sql_stmt_text_debug_and_display() {
        let text = SQLStmtText::new("SELECT 1".to_string());
        assert_eq!(format!("{}", text), "SELECT 1");
        assert!(format!("{:?}", text).contains("SELECT 1"));
    }
}
