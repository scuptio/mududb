#![feature(tuple_trait)]

pub mod common;
pub mod data_type;
pub mod tuple;
pub mod database;
pub mod procedure;
pub mod error;
pub mod log;
pub mod utils;

#[macro_export]
macro_rules! sql_stmt {
    ($expression:expr) => {
        $expression
    };
}
#[macro_export]
macro_rules! sql_param {
    ($expression:expr) => {
        $expression
    };
}
