
pub mod database;
pub mod procedure;
pub mod tuple;


#[macro_export]
macro_rules! sql_stmt {
    ($expression:expr) => {
        $expression
    };
}
#[macro_export]
macro_rules! sql_params {
    ($expression:expr) => {
        $expression
    };
}
