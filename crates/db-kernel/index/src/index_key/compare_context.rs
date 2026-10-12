use mudu::common::result::RS;
use mudu_contract::tuple::comparator::TupleComparator;
use mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc;
use std::cell::RefCell;
use std::ops::Deref;

thread_local! {
    static COMPARE_CONTEXT: RefCell<Option<RefCell<CompareContext>>> = const { RefCell::new(None) };
}

/// Comparator and layout context consulted when comparing index keys.
///
/// A context is installed thread-locally around ordered-map operations (see
/// [`set`](CompareContext::set)) so that the `Ord`/`Eq`/`Hash`
/// implementations of key types can route through the configured comparator;
/// comparator failures are recorded in `result` instead of panicking.
#[derive(Clone)]
pub struct CompareContext {
    /// Outcome of the comparisons performed so far; the first comparator
    /// failure is sticky until the context is re-armed.
    pub result: RS<()>,
    /// Comparator callbacks applied to the raw tuple bytes.
    pub comparator: TupleComparator,
    /// Binary layout of the compared tuples.
    pub desc: TupleBinaryDesc,
}

impl CompareContext {
    /// Runs `f` with the thread-local context. Returns `None` when no context
    /// is installed or when `f` returns `None`.
    pub fn with_context<F, R>(f: F) -> Option<R>
    where
        F: FnOnce(&CompareContext) -> Option<R>,
    {
        Self::with_inner(|c| f(&c.borrow()))
    }

    /// Runs `f` with mutable access to the thread-local context. Returns
    /// `None` when no context is installed or when `f` returns `None`.
    pub fn with_context_mut<F, R>(f: F) -> Option<R>
    where
        F: FnOnce(&mut CompareContext) -> Option<R>,
    {
        Self::with_inner(|c| f(&mut c.borrow_mut()))
    }

    /// Installs `ref_cell` as the thread-local compare context.
    pub fn set(ref_cell: RefCell<CompareContext>) {
        COMPARE_CONTEXT.set(Some(ref_cell))
    }

    /// Removes the thread-local compare context, if any.
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
