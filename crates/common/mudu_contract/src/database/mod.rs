//! Database session, connection, and SQL abstractions.

/// Session context binding a connection, transaction, and result set.
pub mod context;
#[cfg(test)]
mod context_test;
/// Synchronous and asynchronous database connection traits.
pub mod db_conn;
/// Row-mapped entity abstraction.
pub mod entity;
/// Scalar-valued entity abstraction.
pub mod entity_scalar;
#[cfg(test)]
mod entity_scalar_test;
/// Field-level change description.
pub mod field_change;

/// Session context registry and top-level query/command helpers.
pub mod sql;
/// SQL statement abstraction.
pub mod sql_stmt;
#[cfg(test)]
mod sql_stmt_test;
#[cfg(test)]
mod sql_test;

/// A set of entities produced by a query.
pub mod entity_set;
/// Prepared statement abstraction.
pub mod prepared_stmt;
/// A batch of result rows.
pub mod result_batch;
#[cfg(test)]
mod result_batch_test;

#[cfg(test)]
mod entity_set_test;
/// Query result set abstraction.
pub mod result_set;
/// SQL parameter set backed by data values.
pub mod sql_param_value;
#[cfg(test)]
mod sql_param_value_test;
/// SQL parameter set abstraction.
pub mod sql_params;
#[cfg(test)]
mod sql_params_test;
/// SQL statement text wrapper.
pub mod sql_stmt_text;
#[cfg(test)]
mod sql_stmt_text_test;
mod test_command_in;
mod test_entity;
mod test_object;
#[cfg(test)]
mod test_object_test;
/// Transaction abstraction.
pub mod tx;
/// Value-to-host query/command parameter and result types.
pub mod v2h_param;
#[cfg(test)]
mod v2h_param_test;
