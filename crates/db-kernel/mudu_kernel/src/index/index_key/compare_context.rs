use mudu::common::result::RS;
use mudu_contract::tuple::comparator::TupleComparator;
use mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc;
use std::cell::RefCell;
use std::ops::Deref;

thread_local! {
    static COMPARE_CONTEXT: RefCell<Option<RefCell<CompareContext>>> = const { RefCell::new(None) };
}

#[derive(Clone)]
pub struct CompareContext {
    pub result: RS<()>,
    pub comparator: TupleComparator,
    pub desc: TupleBinaryDesc,
}

impl CompareContext {
    pub fn with_context<F, R>(f: F) -> Option<R>
    where
        F: FnOnce(&CompareContext) -> Option<R>,
    {
        Self::with_inner(|c| f(&c.borrow()))
    }

    pub fn with_context_mut<F, R>(f: F) -> Option<R>
    where
        F: FnOnce(&mut CompareContext) -> Option<R>,
    {
        Self::with_inner(|c| f(&mut c.borrow_mut()))
    }

    pub fn set(ref_cell: RefCell<CompareContext>) {
        COMPARE_CONTEXT.set(Some(ref_cell))
    }

    pub fn unset() {
        COMPARE_CONTEXT.set(None)
    }

    fn with_inner<F, R>(f: F) -> Option<R>
    where
        F: FnOnce(&RefCell<CompareContext>) -> Option<R>,
    {
        COMPARE_CONTEXT.with::<_, Option<R>>(|context| {
            let v = context.borrow();
            match v.deref() {
                None => None,
                Some(c) => Some(f(c)?),
            }
        })
    }
}
