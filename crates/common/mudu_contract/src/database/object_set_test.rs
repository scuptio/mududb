#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::database::object::Object;
    use crate::database::object_set::ObjectSet;
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
    fn into_parts_preserves_source_identity_and_unconsumed_rows() {
        let value = TupleValue::from(vec![mudu_type::data_value::DataValue::from_i32(42)]);
        let source: Arc<dyn ResultSet> = Arc::new(MockResultSet::new(vec![value]));
        let desc = make_desc();
        let set = ObjectSet::<i32>::new(source.clone(), desc.clone());
        let (returned_source, returned_desc) = set.into_parts();
        assert!(Arc::ptr_eq(&source, &returned_source));
        assert!(Arc::ptr_eq(&desc, &returned_desc));
        assert_eq!(
            i32::from_tuple_value(&returned_source.next().unwrap().unwrap()).unwrap(),
            42
        );
        assert!(returned_source.next().unwrap().is_none());
    }

    #[test]
    fn object_set_next_record_some_and_none() {
        let value = TupleValue::from(vec![mudu_type::data_value::DataValue::from_i32(7)]);
        let rs = Arc::new(MockResultSet::new(vec![value]));
        let object_set = ObjectSet::<i32>::new(rs, make_desc());

        let first = object_set.next_record().unwrap();
        assert_eq!(first, Some(7));

        let second = object_set.next_record().unwrap();
        assert_eq!(second, None);
    }

    #[test]
    fn object_set_fallible_iterator_next() {
        let value = TupleValue::from(vec![mudu_type::data_value::DataValue::from_i32(9)]);
        let rs = Arc::new(MockResultSet::new(vec![value]));
        let mut object_set = ObjectSet::<i32>::new(rs, make_desc());

        let first = object_set.next().unwrap();
        assert_eq!(first, Some(9));

        let second = object_set.next().unwrap();
        assert_eq!(second, None);
    }

    #[test]
    fn object_set_debug_format() {
        let rs = Arc::new(MockResultSet::new(vec![]));
        let object_set = ObjectSet::<i32>::new(rs, make_desc());
        let output = format!("{:?}", object_set);
        assert!(output.contains("ObjectSet"));
    }
}
