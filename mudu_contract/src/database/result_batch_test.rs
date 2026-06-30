#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::database::entity::Entity;
    use crate::database::result_batch::ResultBatch;
    use crate::database::result_set::{ResultSet, ResultSetAsync};
    use crate::tuple::tuple_field_desc::TupleFieldDesc;
    use crate::tuple::tuple_value::TupleValue;
    use async_trait::async_trait;
    use mudu::common::result::RS;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn empty_row() -> TupleValue {
        TupleValue::from(vec![])
    }

    struct MockResultSet {
        values: Vec<TupleValue>,
        index: AtomicUsize,
    }

    impl MockResultSet {
        fn new(values: Vec<TupleValue>) -> Self {
            Self {
                values,
                index: AtomicUsize::new(0),
            }
        }
    }

    impl ResultSet for MockResultSet {
        fn next(&self) -> RS<Option<TupleValue>> {
            let idx = self.index.fetch_add(1, Ordering::SeqCst);
            Ok(self.values.get(idx).cloned())
        }
    }

    struct MockResultSetAsync {
        values: Vec<TupleValue>,
        index: AtomicUsize,
    }

    impl MockResultSetAsync {
        fn new(values: Vec<TupleValue>) -> Self {
            Self {
                values,
                index: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl ResultSetAsync for MockResultSetAsync {
        async fn next(&self) -> RS<Option<TupleValue>> {
            let idx = self.index.fetch_add(1, Ordering::SeqCst);
            Ok(self.values.get(idx).cloned())
        }

        fn desc(&self) -> &TupleFieldDesc {
            i32::tuple_desc()
        }
    }

    #[test]
    fn result_batch_new_and_add_row() {
        let mut batch = ResultBatch::new(42);
        assert_eq!(batch.oid(), 42);
        assert!(!batch.is_eof());
        assert!(batch.rows().is_empty());

        batch.add_row(empty_row());
        assert_eq!(batch.rows().len(), 1);

        batch.set_eof();
        assert!(batch.is_eof());
    }

    #[test]
    fn result_batch_from_constructors() {
        let rows = vec![empty_row(), empty_row()];
        let batch = ResultBatch::from(7, rows.clone(), true);
        assert_eq!(batch.oid(), 7);
        assert!(batch.is_eof());
        assert_eq!(batch.into_rows().len(), 2);
    }

    #[test]
    fn result_batch_from_result_set() {
        let rows = vec![empty_row(), empty_row()];
        let rs = MockResultSet::new(rows);
        let batch = ResultBatch::from_result_set(1, &rs).unwrap();
        assert_eq!(batch.oid(), 1);
        assert!(batch.is_eof());
        assert_eq!(batch.rows().len(), 2);
    }

    #[test]
    fn result_batch_mut_rows_and_rows() {
        let mut batch = ResultBatch::new(1);
        {
            let rows = batch.mut_rows();
            rows.push(empty_row());
        }
        assert_eq!(batch.rows().len(), 1);
    }

    #[tokio::test]
    async fn result_batch_from_result_set_async() {
        let rows = vec![empty_row(), empty_row()];
        let rs = MockResultSetAsync::new(rows);
        let batch = ResultBatch::from_result_set_async(2, &rs).await.unwrap();
        assert_eq!(batch.oid(), 2);
        assert!(batch.is_eof());
        assert_eq!(batch.rows().len(), 2);
    }
}
