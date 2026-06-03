//! Rust implementation of the `mududb:component-shim/shim-api` world.

wit_bindgen::generate!({
    path: "wit",
    world: "shim-api",
});

mod error;
mod facade;
mod ids;
mod result;
mod statement;
mod value;
mod value_list;

use crate::exports::mududb::component_shim::{system, types};

struct Shim;

impl types::Guest for Shim {
    fn value_null() -> types::Value {
        value::null()
    }

    fn value_from_boolean(input: bool) -> types::Value {
        value::from_boolean(input)
    }

    fn value_from_int64(input: i64) -> types::Value {
        value::from_int64(input)
    }

    fn value_from_float64(input: f64) -> types::Value {
        value::from_float64(input)
    }

    fn value_from_text(input: String) -> types::Value {
        value::from_text(input)
    }

    fn value_from_binary(input: Vec<u8>) -> types::Value {
        value::from_binary(input)
    }

    fn value_from_oid(input: types::Oid) -> types::Value {
        value::from_oid(input)
    }

    fn value_is_null(input: types::Value) -> bool {
        value::is_null(&input)
    }

    fn value_as_boolean(input: types::Value) -> Result<bool, types::Error> {
        value::as_boolean(input)
    }

    fn value_as_int64(input: types::Value) -> Result<i64, types::Error> {
        value::as_int64(input)
    }

    fn value_as_float64(input: types::Value) -> Result<f64, types::Error> {
        value::as_float64(input)
    }

    fn value_as_text(input: types::Value) -> Result<String, types::Error> {
        value::as_text(input)
    }

    fn value_as_binary(input: types::Value) -> Result<Vec<u8>, types::Error> {
        value::as_binary(input)
    }

    fn value_as_oid(input: types::Value) -> Result<types::Oid, types::Error> {
        value::as_oid(input)
    }
}

impl system::Guest for Shim {
    type ValueList = value_list::ValueList;
    type SqlStmt = statement::SqlStmt;
    type ResultSet = result::ResultSet;
    type Row = result::Row;

    fn open(uri: String) -> Result<types::Oid, types::Error> {
        facade::open(&uri)
    }

    fn close(id: types::Oid) -> Result<(), types::Error> {
        facade::close(id)
    }

    fn query(
        id: types::Oid,
        stmt: system::SqlStmt,
        values: system::ValueList,
    ) -> Result<system::ResultSet, types::Error> {
        facade::query(
            id,
            stmt.get::<statement::SqlStmt>(),
            values.get::<value_list::ValueList>(),
        )
        .map(system::ResultSet::new)
    }

    fn command(
        id: types::Oid,
        stmt: system::SqlStmt,
        values: system::ValueList,
    ) -> Result<u64, types::Error> {
        facade::command(
            id,
            stmt.get::<statement::SqlStmt>(),
            values.get::<value_list::ValueList>(),
        )
    }

    fn batch(
        id: types::Oid,
        stmt: system::SqlStmt,
        values: system::ValueList,
    ) -> Result<u64, types::Error> {
        facade::batch(
            id,
            stmt.get::<statement::SqlStmt>(),
            values.get::<value_list::ValueList>(),
        )
    }
}

#[cfg(target_arch = "wasm32")]
export!(Shim);
