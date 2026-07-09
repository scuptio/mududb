#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::database::v2h_param::{
        CommandIn, CommandResult, QueryIn, QueryResult, ResultCursor, ResultRow,
    };
    use crate::tuple::tuple_field::TupleField;
    use crate::tuple::tuple_field_desc::TupleFieldDesc;
    use mudu_type::data_value::DataValue;

    fn empty_desc() -> TupleFieldDesc {
        TupleFieldDesc::new(vec![])
    }

    #[test]
    fn query_in_accessors() {
        let query = QueryIn::new(
            42,
            "SELECT 1".to_string(),
            vec![DataValue::from_i32(1)],
            empty_desc(),
        );
        assert_eq!(query.xid(), 42);
        assert_eq!(query.sql(), "SELECT 1");
        assert_eq!(query.param_list().len(), 1);
        assert!(query.param_desc().is_empty());
    }

    #[test]
    fn query_result_accessors() {
        let desc = empty_desc();
        let result = QueryResult::new(7, desc);
        assert_eq!(result.xid(), 7);
        assert!(result.result_desc().fields().is_empty());

        let cursor = result.cursor();
        assert_eq!(cursor.xid(), 7);

        let desc = result.into_tuple_desc().into();
        assert!(desc.is_empty());
    }

    #[test]
    fn result_cursor_new() {
        let cursor = ResultCursor::new(99);
        assert_eq!(cursor.xid(), 99);
    }

    #[test]
    fn result_row_accessors() {
        let row = ResultRow::new(Some(TupleField::new(vec![vec![1, 2, 3]])));
        assert!(row.result().is_some());

        let row = ResultRow::new(None);
        assert!(row.result().is_none());
        assert!(row.into_result().is_none());
    }

    #[test]
    fn command_in_accessors() {
        let cmd = CommandIn::new(42, "INSERT".to_string(), vec![vec![1, 2, 3]], empty_desc());
        assert_eq!(cmd.xid(), 42);
        assert_eq!(cmd.sql(), "INSERT");
        assert_eq!(cmd.param().len(), 1);
        assert!(cmd.param_desc().fields().is_empty());
    }

    #[test]
    fn command_result_accessors() {
        let result = CommandResult::new(5, 10);
        assert_eq!(result.xid(), 5);
        assert_eq!(result.affected_rows(), 10);
    }
}
