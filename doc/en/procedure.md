# Interactive vs. Procedural: Which Should You Choose?

Interactive and procedural approaches are two distinct ways to build database applications.

## Interactive Approach

With the interactive approach, users execute SQL statements directly through command-line tools, GUI tools, client
libraries, or ORM frameworks.

**Advantages**:

- **Immediate feedback**: View results instantly.

- **Rapid prototyping**: Ideal for exploration and debugging.

- **Simple workflow**: Minimal setup required.

- **Beginner-friendly**: Gentle learning curve.

**Disadvantages**:

- **Poor performance**: Communication overhead between DB client and server.

- **Correctness challenges**: Vulnerable transaction semantics.

## Procedural Approach

With the procedural approach, developers implement business logic with stored procedures, functions, and triggers.

**Advantages**:

- **Performance optimization**: Reduced network overhead.

- **Code reusability**: Centralized business logic.

- **Transaction control**: Better ACID compliance.

- **Enhanced security**: Reduced SQL injection risks.

**Disadvantages**:

- **Steep learning curve**: Requires DB-specific procedural languages.

- **Debugging difficulties**: Harder to troubleshoot.

- **Vendor lock-in**: Limited portability between DBMS.

- **Version control challenges**: Requires specialized tools.

---

# Mudu Procedure (MP): Unified Interactive and Procedural Execution

The same code can run in both interactive and procedural modes.

MP is designed to combine the strengths of both approaches while avoiding their typical drawbacks. You can write
Mudu Procedures in most modern languages, without relying on database-specific procedural syntax such as PostgreSQL
PL/pgSQL or MySQL stored procedures.

During development, Mudu Procedures run interactively, much like code built on top of an ORM framework.

## Current Implementation (Rust)

Mudu Runtime currently supports Rust. A Rust-based procedure uses the following function signature:

## Procedure specification

``` 
#[mudu_proc]
fn {procedure_name}(
    xid: XID,
    {argument_list...}
) -> RS<{return_value_type}>
```

### {procedure_name}:

A valid Rust function name.

### Macro #[mudu_proc]:

Marks the function as a Mudu procedure.

### Parameters:

#### xid:

The transaction ID.

### {argument_list...}:

Input arguments that implement the `Entity` trait.

Supported types: `i32`, `i64`, `String`, `f32`, `f64`.

Unsupported: Custom structs, enums, arrays, or tuples.

### Return value:

#### {return_value_type}:

The return type must implement the [`Entity` trait](../lang.common/proc_key_traits.md). The supported types are the
same as the argument types.

The result type alias `RS` is defined as:

```rust
use mudu::error::error::ER;
pub type RS<X> = Result<X, ER>;  // ER: Error
```

## CRUD (Create/Read/Update/Delete) Operations in Mudu Procedures

There are two key APIs that a Mudu procedure can invoke:

### 1. `query`

`query` is used for `SELECT` statements.

<!--
quote_begin
content="[Query API](../lang.common/mudu_query.md#L-L)"
-->
<!--
quote_begin
content="[Query API](../../sys_interface/src/api.rs#L34-L40)"
lang="rust"
-->

```rust
pub fn mudu_query<
    R: Entity
>(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    inner::inner_query(xid, sql, params)
}
```

<!--quote_end-->
<!--quote_end-->

`query` automatically performs R2O (relation-to-object) mapping and returns a result set of objects that implement
the `Entity` trait.

### 2. `command`

`command` is used for `INSERT`, `UPDATE`, and `DELETE` statements.

<!--
quote_begin
content="[Command API](../lang.common/mudu_command.md#L-L)"
-->
<!--
quote_begin
content="[Command API](../../sys_interface/src/api.rs#L11-L19)"
lang="rust"
-->

```rust
pub fn mudu_command(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    inner::inner_command(xid, sql, params)
}
```

<!--quote_end-->
<!--quote_end-->

### Parameters for Both

#### xid:

The transaction ID.

#### sql:

An SQL statement that uses `?` as parameter placeholders.

#### params:

The parameter list.


<!--
quote_begin
content="[KeyTrait](../lang.common/proc_key_traits.md#L-L)"
-->

## Key Traits

### SQLStmt

<!--
quote_begin
content="[Entity](../../mudu/src/database/entity.rs#L12-L34)"
lang="rust"
-->

```rust
pub trait Entity: private::Sealed + Datum {
    fn new_empty() -> Self;

    fn tuple_desc() -> &'static TupleFieldDesc;

    fn object_name() -> &'static str;

    fn get_field_binary(&self, field_name: &str) -> RS<Option<Vec<u8>>>;

    fn set_field_binary<B: AsRef<[u8]>>(&mut self, field_name: &str, binary: B) -> RS<()>;

    fn get_field_value(&self, field_name: &str) -> RS<Option<DatValue>>;

    fn set_field_value<D: AsRef<DatValue>>(&mut self, field_name: &str, value: D) -> RS<()>;

    fn from_tuple(tuple_row: &TupleField) -> RS<Self> {
        entity_utils::entity_from_tuple(tuple_row)
    }

    fn to_tuple(&self) -> RS<TupleField> {
        entity_utils::entity_to_tuple(self)
    }
}
```

<!--quote_end-->


<!--
quote_begin
content="[SQLStmt](../../mudu/src/database/sql_stmt.rs#L3-L8)"
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
content="[DatumDyn](../../mudu/src/data_type/datum.rs#L18-L38)"
lang="rust"
-->

```rust
pub trait Datum: DatumDyn + Clone + 'static {
    fn dat_type() -> &'static DatType;

    fn from_binary(binary: &[u8]) -> RS<Self>;

    fn from_value(value: &DatValue) -> RS<Self>;

    fn from_textual(textual: &str) -> RS<Self>;
}

pub trait DatumDyn: fmt::Debug + Send + Sync + Any {
    fn dat_type_id(&self) -> RS<DatTypeID>;

    fn to_binary(&self, dat_type: &DatType) -> RS<DatBinary>;

    fn to_textual(&self, dat_type: &DatType) -> RS<DatTextual>;

    fn to_value(&self, dat_type: &DatType) -> RS<DatValue>;

    fn clone_boxed(&self) -> Box<dyn DatumDyn>;
}
```

<!--quote_end-->
<!--quote_end-->

## Example: A Wallet App Transfer Procedure

<!--
quote_begin
content="[Example](../lang.common/transfer_funds.md#L-L)"
-->
<!--
quote_begin
content="[Transfer](../../example/wallet/src/rust/procedures.rs#L23-L104)"
lang="rust"
-->
```rust
#[mudu_proc]
pub fn transfer_funds(xid: XID, from_user_id: i32, to_user_id: i32, amount: i32) -> RS<()> {
    // Check amount > 0
    if amount <= 0 {
        return Err(m_error!(
            MuduError,
            "The transfer amount must be greater than 0"
        ));
    }

    // Cannot transfer money to oneself
    if from_user_id == to_user_id {
        return Err(m_error!(MuduError, "Cannot transfer money to oneself"));
    }

    // Check whether the transfer-out account exists and has sufficient balance
    let wallet_rs = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT user_id, balance FROM wallets WHERE user_id = ?;"),
        sql_params!(&from_user_id),
    )?;

    let from_wallet = if let Some(row) = wallet_rs.next_record()? {
        row
    } else {
        return Err(m_error!(MuduError, "no such user"));
    };

    if *from_wallet.get_balance().as_ref().unwrap() < amount {
        return Err(m_error!(MuduError, "insufficient funds"));
    }

    // Check the user account existing
    let to_wallet = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT user_id FROM wallets WHERE user_id = ?;"),
        sql_params!(&(to_user_id)),
    )?;
    let _to_wallet = if let Some(row) = to_wallet.next_record()? {
        row
    } else {
        return Err(m_error!(MuduError, "no such user"));
    };

    // Perform a transfer operation
    // 1. Deduct the balance of the account transferred out
    let deduct_updated_rows = mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance - ? WHERE user_id = ?;"),
        sql_params!(&(amount, from_user_id)),
    )?;
    if deduct_updated_rows != 1 {
        return Err(m_error!(MuduError, "transfer fund failed"));
    }
    // 2. Increase the balance of the transfer-in account
    let increase_updated_rows = mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = balance + ? WHERE user_id = ?;"),
        sql_params!(&(amount, to_user_id)),
    )?;
    if increase_updated_rows != 1 {
        return Err(m_error!(MuduError, "transfer fund failed"));
    }

    // 3. Entity the transaction
    let id = Uuid::new_v4().to_string();
    let insert_rows = mudu_command(
        xid,
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
        return Err(m_error!(MuduError, "transfer fund failed"));
    }
    Ok(())
}
```
<!--quote_end-->
<!--quote_end-->

## MP and Transactions

Mudu procedures support two transaction execution modes:

### Automatic Mode

Each procedure runs as an independent transaction. The transaction:

- Commits automatically if the procedure returns `Ok`

- Rolls back automatically if the procedure returns `Err`

### Manual Mode

Pass a transaction ID (`xid`) across multiple Mudu procedures for explicit transaction control.

#### Example:

```
procedure1(xid);
procedure2(xid);
commit(xid); // Explicit commit
// or rollback(xid) for explicit rollback
```

# Benefits of Using Mudu Procedures

## 1. Single Codebase for Both Modes

"Develop once!"

Mudu Procedures use the exact same code for both interactive development and production deployment. This eliminates
context switching between tools and ensures consistency across environments.

## 2. Native ORM Support

Seamless object-relational mapping.
The framework provides built-in ORM capabilities through the `Entity` trait. It automatically maps query results to
Rust structs, eliminating boilerplate conversion code while preserving type safety.

## 3. Static Analysis Friendly

Better validation for AI-generated code.

Mudu's strongly-typed API enables:

1. Compile-time checks for SQL syntax via the `sql_stmt!` macro

2. Type validation of parameters and return values

3. Early error detection for AI-generated code, which is critical for reliability

## 4. Data Proximity Processing

Major efficiency gains.

Execute data transformations directly in the database.
One example is preparing AI training datasets without export/import steps.

```rust
// Prepare AI training dataset without export/import
#[mudu_proc]
fn prepare_training_data(xid: XID) -> RS<()> {
    mudu_command(xid,
        sql_stmt!("..."),
        sql_param!(&[]))?;
    // Further processing...
}
```

Benefit: Faster processing for large datasets by avoiding network transfer.

## 5. Extended Database Capabilities

Leverage full programming ecosystems.
You can use any Rust crate today, and potentially other language ecosystems in the future.

For example, you can use the `uuid` and `chrono` crates:

```rust
use chrono::Utc;
use uuid::Uuid;

#[mudu_proc]
fn create_order(xid: XID, user_id: i32) -> RS<String> {
    // Do something ....

    let order_id = Uuid::new_v4().to_string();
    let created_at = Utc::now().naive_utc();
    
    mudu_command(xid,
        sql_stmt!("INSERT INTO orders (id, user_id, created_at) 
                   VALUES (?, ?, ?)"),
        sql_param!(&[&order_id, &user_id, &created_at]))?;
    
    // Do something ....

    Ok(order_id)
}
```

Advantages:

1. Use specialized libraries such as UUID, datetime, and geospatial libraries

2. Implement complex logic that is difficult or impossible in pure SQL

3. Manage dependencies through familiar tools such as Cargo, npm, or pip

# Key Technical Advantages

| Feature         | Traditional Approach       | MP Advantage              |
|:----------------|:---------------------------|:--------------------------|
| Dev-Prod Parity | Different code for CLI/SPs | Identical codebase        |
| Type Safety     | Runtime SQL errors         | Compile-time validation   |
| Data Movement   | ETL pipelines required     | In-database processing    |
| Extensibility   | DB-specific extensions     | General-purpose libraries |

# How MuduDB Treats the Interactive and Procedural Approach Uniformly

Unlike traditional monolithic databases, MuduDB is split into two components: Mudu Runtime and the DB Kernel.

The kernel provides the core foundations, transaction management, and storage capabilities.
The runtime supports a multi-language ecosystem.
It can host a VM (Virtual Machine) and execute WASM bytecode modules compiled from mainstream programming languages.
During MP execution, the runtime collaborates with the kernel to complete the procedure.

To illustrate this, consider the following example.
Suppose a procedure executes queries `Q1` and `Q2`, a command `C1`, and functions `T1` and `T2`, which are written in
a high-level language and compiled to bytecode.

```
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
