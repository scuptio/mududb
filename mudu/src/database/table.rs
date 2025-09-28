use crate::common::result::RS;
use crate::database::context::Context;
use crate::database::predicate::Predicate;
use crate::database::project::Project;
use crate::database::record::Record;
use std::marker::PhantomData;

pub struct Iter<R: Record> {
    phantom: PhantomData<R>,
}

pub trait Iterator {
    type Item;

    fn next(&self) -> RS<Option<Self::Item>> {
        unimplemented!()
    }
}
impl<R: Record> Iter<R> {
    pub fn new() -> Self {
        Self { phantom: Default::default() }
    }
}

impl<R: Record> Iterator for Iter<R> {
    type Item = R;

    fn next(&self) -> RS<Option<Self::Item>> {
        unimplemented!()
    }
}

pub trait Table<R: Record> {
    fn table_name() -> &'static str;

    fn query(&self, context: &Context, predicate: &Predicate, project: &Project) -> RS<Iter<R>>;

    fn insert(&self, context: &Context, tuple: R) -> RS<()>;

    fn update(&self, context: &Context, tuple: R, key_predicate: &Predicate) -> RS<()>;

    fn delete(&self, context: &Context, key_predicate: &Predicate) -> RS<()>;
}