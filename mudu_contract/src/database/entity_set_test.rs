#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::database::entity::Entity;
    use crate::database::entity_set::RecordSet;
    use crate::database::result_set::ResultSet;
    use crate::tuple::tuple_field_desc::TupleFieldDesc;
    use crate::tuple::tuple_value::TupleValue;
    use fallible_iterator::FallibleIterator;
    use mudu::common::result::RS;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    fn make_desc() -> Arc<TupleFieldDesc> {
        Arc::new(i32::tuple_desc().clone())
    }

    #[test]
    fn record_set_next_record_some_and_none() {
        let value = TupleValue::from(vec![mudu_type::dat_value::DatValue::from_i32(7)]);
        let rs = Arc::new(MockResultSet::new(vec![value]));
        let record_set = RecordSet::<i32>::new(rs, make_desc());

        let first = record_set.next_record().unwrap();
        assert_eq!(first, Some(7));

        let second = record_set.next_record().unwrap();
        assert_eq!(second, None);
    }

    #[test]
    fn record_set_fallible_iterator_next() {
        let value = TupleValue::from(vec![mudu_type::dat_value::DatValue::from_i32(9)]);
        let rs = Arc::new(MockResultSet::new(vec![value]));
        let mut record_set = RecordSet::<i32>::new(rs, make_desc());

        let first = record_set.next().unwrap();
        assert_eq!(first, Some(9));

        let second = record_set.next().unwrap();
        assert_eq!(second, None);
    }

    #[test]
    fn record_set_debug_format() {
        let rs = Arc::new(MockResultSet::new(vec![]));
        let record_set = RecordSet::<i32>::new(rs, make_desc());
        let output = format!("{:?}", record_set);
        assert!(output.contains("RecordSet"));
    }
}
