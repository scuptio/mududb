//! `database::mod` module.
#![allow(missing_docs)]

pub mod context;
#[cfg(test)]
mod context_test;
pub mod db_conn;
pub mod entity;
pub mod entity_scalar;
#[cfg(test)]
mod entity_scalar_test;
pub mod field_change;

pub mod sql;
pub mod sql_stmt;
#[cfg(test)]
mod sql_stmt_test;
#[cfg(test)]
mod sql_test;

pub mod entity_set;
pub mod prepared_stmt;
pub mod result_batch;
#[cfg(test)]
mod result_batch_test;

#[cfg(test)]
mod entity_set_test;
pub mod result_set;
pub mod sql_param_value;
#[cfg(test)]
mod sql_param_value_test;
pub mod sql_params;
#[cfg(test)]
mod sql_params_test;
pub mod sql_stmt_text;
#[cfg(test)]
mod sql_stmt_text_test;
mod test_command_in;
mod test_entity;
mod test_object;
#[cfg(test)]
mod test_object_test;
pub mod tx;
pub mod v2h_param;
#[cfg(test)]
mod v2h_param_test;
