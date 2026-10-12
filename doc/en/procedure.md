# Mudu Procedure specification and development guide

## Background

Interactive and procedural approaches are two distinct ways to build database applications.

With the interactive approach, users execute SQL statements directly through command-line tools, GUI tools, client
libraries, or ORM frameworks. It gives immediate feedback, suits rapid prototyping and debugging, needs minimal
setup, and is beginner-friendly; in exchange it pays communication overhead between the DB client and server, and
its transaction semantics are vulnerable to correctness mistakes.

With the procedural approach, developers implement business logic with stored procedures, functions, and triggers.
It reduces network overhead, centralizes business logic, improves transaction control (better ACID compliance), and
reduces SQL injection risk; in exchange it requires database-specific procedural languages, is harder to debug,
risks vendor lock-in, and complicates version control.

## Mudu Procedure (MP): Unified Interactive and Procedural Execution

MP is designed to combine the strengths of both approaches while avoiding their typical drawbacks. The same code
can run in both interactive and procedural modes. You can write Mudu Procedures in most modern languages, without
relying on database-specific procedural syntax such as PostgreSQL PL/pgSQL or MySQL stored procedures. During
development, Mudu Procedures run interactively, much like code built on top of an ORM framework.

### Guest languages

Mudu Procedures are compiled to WASM components, and the runtime hosts six guest languages: Rust, AssemblyScript,
C#, Python, C, and Go. This guide uses the Rust facade, the reference implementation of the procedure API; the
other languages are covered by the [binding API surface](../dev/binding_api_surface.md) and the
[SDK distribution guide](../dev/sdk_distribution.md).

A Rust-based procedure uses the following function signature:

### Procedure specification

```text
/**mudu-proc**/
fn {procedure_name}(
    oid: OID,
    {argument_list...}
) -> RS<{return_value_type}>
```

#### Procedure name:

A valid Rust function name.

#### Marker `/**mudu-proc**/`:

Marks the following function as a Mudu Procedure for the transpiler.

#### Parameters:

##### oid:

The current system session ID passed into the procedure.

#### {argument_list...}:

Input arguments must be representable by Mudu's datum / tuple conversion system.

Commonly used supported forms include scalar values such as `i32`, `i64`, `String`, `f32`, and `f64`, as well as
container forms used by current examples such as `Vec<String>`.

The exact supported surface is defined by the current
[`mudu_type` conversion](../../crates/common/mudu_type/src/datum.rs) and transpiler implementation rather than a
fixed short whitelist in this document.

#### Return value:

##### {return_value_type}:

The return value must also be representable by Mudu's datum / tuple conversion system.
In current implementations this includes scalar values, tuple return values, and `Vec<T>` style result containers.

The result type alias `RS` is defined as:

```rust
use mudu::error::err::MError;
pub type RS<X> = Result<X, MError>;
```

Inside a procedure, the runtime can call [system calls](syscall.md), such as SQL system calls (`mudu_query` / `mudu_command` / `mudu_batch`) or key/value system calls (`mudu_get` / `mudu_put` / `mudu_range`).

### CRUD (Create/Read/Update/Delete) Operations in Mudu Procedures

There are two key APIs that a Mudu Procedure can invoke.
For stable imports, handwritten synchronous procedure code should use `sys_interface::sync_api`, while generated async
procedure code should use `sys_interface::async_api`.

#### 1. `query`

`query` is used for `SELECT` statements.

<!--
quote_begin
content="[Query API](../lang.common/mudu_query.md#L-L)"
-->
```rust
use sys_interface::sync_api::mudu_query;
use sys_interface::async_api::mudu_query as mudu_query_async;

pub fn mudu_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<EntitySet<R>> {
    /* ... */
}

pub async fn mudu_query_async<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<EntitySet<R>> {
    /* ... */
}
```

<!--quote_end-->

`query` automatically performs R2O (relation-to-object) mapping and returns a result set of objects that implement
the `Entity` trait.

#### 2. `command`

`command` is used for `INSERT`, `UPDATE`, and `DELETE` statements.

<!--
quote_begin
content="[Command API](../lang.common/mudu_command.md#L-L)"
-->
```rust
use sys_interface::sync_api::mudu_command;
use sys_interface::async_api::mudu_command as mudu_command_async;

pub fn mudu_command(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    /* ... */
}

pub async fn mudu_command_async(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    /* ... */
}
```

<!--quote_end-->

#### 3. `batch`

`batch` is the SQL batch variant of `command`. It keeps the same API surface and return shape, but it is intended for
raw batch SQL text executed by the runtime through `libsql::execute_batch`.

<!--
quote_begin
content="[Batch API](../lang.common/mudu_batch.md#L-L)"
-->
```rust
use sys_interface::sync_api::mudu_batch;
use sys_interface::async_api::mudu_batch as mudu_batch_async;

pub fn mudu_batch(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    /* ... */
}

pub async fn mudu_batch_async(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    /* ... */
}
```
<!--quote_end-->

#### Parameters for Both

##### oid:

The current system session identifier passed into the procedure.

##### sql:

An SQL statement that uses `?` as parameter placeholders.

##### params:

The parameter list.
For `batch`, the current libsql-backed implementation requires this list to be empty.

#### Generated entity bindings (`mgen entity`)

For each table in `sql/ddl.sql`, `mgen entity` generates a Rust module containing:

- the row struct (e.g. `Wallets`) implementing the `Entity` trait, plus `TABLE_NAME` and `columns::*` name
  constants;
- pre-built SQL text constants `SQL_GET_BY_PK`, `SQL_INSERT`, and `SQL_DELETE_BY_PK` for tables with a primary
  key, and `insert_params()` returning the typed bind tuple for `SQL_INSERT`;
- a partial-update changeset `<Table>Change` holding one `FieldChange<T>` per non-key column: `Unchanged`
  (the default) leaves the column out of the update, `Set(v)` sets it, and for a nullable column `Set(None)`
  writes a SQL `NULL`.

`update_by_pk(...)` builds the `UPDATE ... SET ... WHERE <pk> = ?` statement and its bind parameters, and
returns `None` when no column is marked:

```rust
let change = WalletsChange {
    balance: FieldChange::Set(Some(200)),
    ..WalletsChange::default()
};
if let Some((sql, params)) = change.update_by_pk(user_id) {
    mudu_command(oid, sql_stmt!(&sql), sql_params!(&params))?;
}
```

SQL literals in procedure sources can be checked against the DDL at build time with `mgen check-sql`;
see [`../dev/sql_subset.md`](../dev/sql_subset.md).


<!--
quote_begin
content="[KeyTrait](../lang.common/proc_key_traits.md#L-L)"
-->

## Key Traits

### SQLStmt

<!--
quote_begin
content="[Entity](../../crates/common/mudu_contract/src/database/entity.rs#L23-L44)"
lang="rust"
-->

```rust
pub trait Entity: Datum {
    fn tuple_desc() -> &'static TupleFieldDesc;

    fn table_name() -> &'static str;

    fn from_tuple(row: &TupleField) -> RS<Self>;

    fn from_tuple_value(row: &TupleValue) -> RS<Self>;

    fn to_tuple(&self) -> RS<TupleField>;

    fn to_tuple_value(&self) -> RS<TupleValue>;
}
```

<!--quote_end-->


<!--
quote_begin
content="[SQLStmt](../../crates/common/mudu_contract/src/database/sql_stmt.rs#L6-L10)"
lang="rust"
-->

```rust
pub trait SQLStmt: fmt::Debug + fmt::Display + Sync + Send {
    fn to_sql_string(&self) -> String;

    fn clone_boxed(&self) -> Box<dyn SQLStmt>;
}
```

<!--quote_end-->

### Datum, DatumDyn

<!--
quote_begin
content="[DatumDyn](../../crates/common/mudu_type/src/datum.rs#L20-L40)"
lang="rust"
-->

```rust
pub trait Datum: DatumDyn + Clone + 'static {
    fn data_type() -> DataType;

    fn from_binary(binary: &[u8]) -> RS<Self>;

    fn from_value(value: &DataValue) -> RS<Self>;

    fn from_textual(textual: &str) -> RS<Self>;
}

pub trait DatumDyn: fmt::Debug + Send + Sync + Any {
    fn type_family(&self) -> RS<TypeFamily>;

    fn to_binary(&self, data_type: &DataType) -> RS<DataBinary>;

    fn to_textual(&self, data_type: &DataType) -> RS<DataTextual>;

    fn to_value(&self, data_type: &DataType) -> RS<DataValue>;

    fn clone_boxed(&self) -> Box<dyn DatumDyn>;
}
```

<!--quote_end-->
<!--quote_end-->

### Example: A Wallet App Transfer Procedure

<!--
quote_begin
content="[Example](../lang.common/transfer_funds.md#L-L)"
-->
<!--
quote_begin
content="[Transfer](../../example/mpk/wallet/src/rust/procedures.rs#L23-L104)"
lang="rust"
-->
```rust
/**mudu-proc**/
pub fn transfer_funds(oid: OID, from_user_id: i32, to_user_id: i32, amount: i32) -> RS<()> {
    // Check amount > 0
    if amount <= 0 {
        return Err(mudu_error!(
            MuduError,
            "The transfer amount must be greater than 0"
        ));
    }

    // Cannot transfer money to oneself
    if from_user_id == to_user_id {
        return Err(mudu_error!(MuduError, "Cannot transfer money to oneself"));
    }

    // Check whether the transfer-out account exists and has sufficient balance
    let wallet_rs = mudu_query::<Wallets>(
        oid,
        sql_stmt!(&"SELECT user_id, balance FROM wallets WHERE user_id = ?;"),
        sql_params!(&from_user_id),
    )?;

    let from_wallet = if let Some(row) = wallet_rs.next_record()? {
        row
    } else {
        return Err(mudu_error!(MuduError, "no such user"));
    };

    if *from_wallet.get_balance().as_ref().unwrap() < amount {
        return Err(mudu_error!(MuduError, "insufficient funds"));
    }

    // Check the user account existing
    let to_wallet = mudu_query::<Wallets>(
        oid,
        sql_stmt!(&"SELECT user_id FROM wallets WHERE user_id = ?;"),
        sql_params!(&(to_user_id)),
    )?;
    let _to_wallet = if let Some(row) = to_wallet.next_record()? {
        row
    } else {
        return Err(mudu_error!(MuduError, "no such user"));
    };

    // Perform a transfer operation
    // 1. Deduct the balance of the account transferred out
    let deduct_updated_rows = mudu_command(
        oid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ? WHERE user_id = ?;"),
        sql_params!(&(amount, from_user_id)),
    )?;
    if deduct_updated_rows != 1 {
        return Err(mudu_error!(MuduError, "transfer fund failed"));
    }
    // 2. Increase the balance of the transfer-in account
    let increase_updated_rows = mudu_command(
        oid,
        sql_stmt!(&"UPDATE wallets SET balance = balance + ? WHERE user_id = ?;"),
        sql_params!(&(amount, to_user_id)),
    )?;
    if increase_updated_rows != 1 {
        return Err(mudu_error!(MuduError, "transfer fund failed"));
    }

    // 3. Entity the transaction
    let id = Uuid::new_v4().to_string();
    let insert_rows = mudu_command(
        oid,
        sql_stmt!(
            &r#"
        INSERT INTO transactions
        (trans_id, from_user, to_user, amount)
        VALUES (?, ?, ?, ?);
        "#
        ),
        sql_params!(&(id, from_user_id, to_user_id, amount)),
    )?;
    if insert_rows != 1 {
        return Err(mudu_error!(MuduError, "transfer fund failed"));
    }
    Ok(())
}
```
<!--quote_end-->
<!--quote_end-->

### MP and Transactions

Mudu Procedures support two transaction execution modes:

#### Automatic Mode

Each procedure runs as an independent transaction. The transaction:

- Commits automatically if the procedure returns `Ok`

- Rolls back automatically if the procedure returns `Err`

#### Manual Mode

Passing a shared session identifier (`oid`) across multiple Mudu Procedures for explicit transaction control is a
planned capability. The current procedure API exposes no `commit` / `rollback`: each procedure invocation runs as
an independent transaction, which the runtime commits when the procedure returns `Ok` and rolls back when it
returns `Err` (see Automatic Mode above). The pseudocode below is illustrative only.

##### Example (illustrative pseudocode):

```text
procedure1(oid);
procedure2(oid);
commit(oid); // Explicit commit
// or rollback(oid) for explicit rollback
```

## Development constraints

`Entity` maps query results to Rust structs. The build's `mgen check-sql` step checks SQL against DDL, and the language compiler checks parameter and return types. These checks catch some errors but do not establish correct transaction logic, permissions or runtime behavior. Both handwritten and AI-generated procedures need tests.

Moving several operations into a procedure can reduce client round trips. Performance still depends on query cost, data volume and procedure execution overhead; measure it with the intended workload.

Procedures can reuse Rust crates that support `wasm32-wasip2` and the host interface. Check compatibility separately for libraries that depend on native system calls or shared libraries. Access operations such as randomness and time through the interfaces provided by `mududb::sys`.

## How MuduDB Treats the Interactive and Procedural Approach Uniformly

MuduDB is split into two components: Mudu Runtime and the DB Kernel.

The kernel provides the core foundations, transaction management, and storage capabilities.
The runtime supports a multi-language ecosystem.
It can host a VM (Virtual Machine) and execute WASM bytecode modules compiled from mainstream programming languages.
During MP execution, the runtime collaborates with the kernel to complete the procedure.

To illustrate this, consider the following example.
Suppose a procedure executes queries `Q1` and `Q2`, a command `C1`, and functions `T1` and `T2`, which are written in
a high-level language and compiled to bytecode.

```text
procedure {
    query Q1
    do something T1
    query Q2
    do something T2
    command C1
}
```

The following figures show the difference between the two approaches.

<div align="center">
<img src="../pic/interactive_tx.png" width="20%">
&nbsp&nbsp&nbsp&nbsp
<img src="../pic/procedural_tx.png" width="26%">   
</div>
