use crate::database::context::Context;
use crate::database::entity::Entity;
use crate::database::predicate::Predicate;
use crate::database::project::Project;
use mudu::common::result::RS;
use std::marker::PhantomData;

pub struct Iter<R: Entity> {
    phantom: PhantomData<R>,
}

pub trait Iterator {
    type Item;

    fn next(&self) -> RS<Option<Self::Item>> {
        unimplemented!()
    }
}
impl<R: Entity> Iter<R> {
    pub fn new() -> Self {
        Self {
            phantom: Default::default(),
        }
    }
}

impl<R: Entity> Iterator for Iter<R> {
    type Item = R;

    fn next(&self) -> RS<Option<Self::Item>> {
        unimplemented!()
    }
}

pub trait Table<R: Entity> {
    fn table_name() -> &'static str;

    fn query(&self, context: &Context, predicate: &Predicate, project: &Project) -> RS<Iter<R>>;

    fn insert(&self, context: &Context, tuple: R) -> RS<()>;

    fn update(&self, context: &Context, tuple: R, key_predicate: &Predicate) -> RS<()>;

    fn delete(&self, context: &Context, key_predicate: &Predicate) -> RS<()>;
}
