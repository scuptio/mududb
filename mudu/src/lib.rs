pub mod common;
pub mod data_type;
pub mod tuple;
pub mod database;
#[macro_export]
macro_rules! sql_stmt {
    ($expression:expr) => {
        $crate::database::sql::function_sql_stmt($expression)
    };
}
#[macro_export]
macro_rules! sql_param {
    ($expression:expr) => {
        $crate::database::sql::function_sql_param($expression)
    };
}
