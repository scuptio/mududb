# Interactive vs. Procedural: Which Is Your Choice?

Interactive and procedural approaches represent two distinct methods for developing database applications.

## Interactive Approach:

When using the interactive approach, users directly execute SQL statements via command-line or GUI tools, or utilize
client libraries or ORM mapping frameworks.

**Advantages**:

- **Immediate feedback**: View results instantly.

- **Rapid prototyping**: Ideal for exploration and debugging.

- **Simple workflow**: Minimal setup required.

- **Beginner-friendly**: Gentle learning curve.

**Disadvantages**:

- **Poor performance**: Communication overhead between DB client and server.

- **Correctness challenges**: Vulnerable transaction semantics.

## Procedural Approach

In the procedural approach, developers implement business logic using stored procedures, functions, and triggers.

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

# Mudu Portable Data Access(MPDA) Code: Unified Interactive and Procedural Execution

One piece of code can run both interactively and procedurally.

We aim to combine the advantages of both modes while eliminating their drawbacks. MPDA achieves this. You can
write Mudu Procedures in most modern languages—without relying on "weird" or "ugly" syntax like PostgreSQL PL/pgSQL or
MySQL’s stored procedures.

During development, Mudu Procedures run interactively like an ORM mapping framework.

## Current Implementation (Rust)

Mudu Runtime currently supports Rust. A Rust-based stored procedure uses the following function signature:

## Procedure specification

```
#[mudu_proc]
fn {procedure_name}(
    xid: XID,
    {argument_list...}
) -> RS<{return_value_type}>
```

### {procedure_name}:

Valid Rust function name.

### Macro #[mudu_proc]:

Macro identifying the function as a Mudu procedure.

### Parameters:

#### xid:

Transaction ID.

### {argument_list...}:

Input arguments implementing the `Entity` trait.

Supported types:  `i32`, `i64`, `String`, `f32`, `f64`.

Unsupported: Custom structs, enums, arrays, or tuples.

### Return value:

#### {return_value_type}:

Return type implementing the `Entity` trait (same supported types as arguments).

Return Result Type `RS` is `Result` enum:

```rust
use mudu::error::error::ER;
pub type RS<X> = Result<X, ER>;  // ER: Error
```

## CRUD(Create/Read/Update/Delete) Operations in Mudu Procedures

There are two key APIs that a Mudu procedure can invoke:

### 1. `query`

`query` for SELECT statements

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

`query` Performs R2O(relation to object) mapping automatically, returning a result set of objects implementing the
`Entity` trait.

### 2. `command`

`command` for INSERT/UPDATE/DELETE

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

### Parameters for Both:

#### xid:

Transaction ID.

#### sql:

SQL statement with ? as parameter placeholders.

#### params:

Parameter list.


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

## A Example: A Wallet APP's Transfer Procedure

<!--
quote_begin
content="[Example](../lang.common/transfer_funds.md#L-L)"
-->
<!--
quote_begin
content="[Transfer](../../example/wallet/src/rust/procedures.rs#L23-L105)"
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

## MPDA and Transaction

Mudu procedure supports 2 transaction execution modes:

### Automatic Mode

Each procedure runs as an independent transaction. The transaction:

- Commits automatically if the procedure returns Ok

- Rollback automatically if the procedure returns Err

### Manual Mode

Pass a transaction ID (xid) across multiple Mudu procedures for explicit transaction control.

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

Seamless object-relational mapping
The framework provides built-in ORM capabilities through the Entity trait. It automatically maps query results to Rust
structs, eliminating boilerplate conversion code while maintaining type safety.

## 3. Static Analysis Friendly

AI-generated code validation

Mudu's strongly-typed API enables:

1. Compile-time checks for SQL syntax via sql_stmt! macro

2. Type validation of parameters and return values

3. Early error detection for AI-generated code (critical for reliability)

## 4. Data Proximity Processing

Massive efficiency gains。

Execute data transformations directly in the database.
An example is preparing AI training dataset without export/import.

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

Benefit: Faster for large datasets by avoiding network transfer.

### 5. Extended Database Capabilities

Leverage full programming ecosystems
Tap into any Rust crate (or future language ecosystems):

Example, use `uuid` and `chrono` crate,

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

1. Use specialized libraries (UUID, datetime, geospatial, etc.)

2. Implement complex logic impossible in pure SQL

3. Maintain dependency management through Cargo/npm/pip

# Key Technical Advantages

| Feature         | Traditional Approach       | MPDA Advantage            |
|:----------------|:---------------------------|:--------------------------|
| Dev-Prod Parity | Different code for CLI/SPs | Identical codebase        |
| Type Safety     | Runtime SQL errors         | Compile-time validation   |
| Data Movement   | ETL pipelines required     | In-database processing    |
| Extensibility   | DB-specific extensions     | General-purpose libraries |

# How MuduDB Treats the Interactive and Procedural Approach Uniformly

MuduDB differs from traditional monolithic-architecture databases by splitting into two components: Mudu Runtime and DB
Kernel.

Kernel provides basis foundations, transactions, and storage capabilities.
Runtime supports for multi-language ecosystems.
This runtime can host a VM(Virtual Machine) and execute intermediate WASM bytecode modules, into which mainstream
programming languages can be compiled.
During a MPDA execution, the runtime collaborates with kernel to complete the process.
To illustrate this point, consider the following example:
Suppose a procedure executes queries Q1, Q2, condition C1, and functions T1, T2 (implemented in a high-level language
and can be compiled to the bytecode).

```
procedure {
    query Q1
    do something T1
    query Q2
    do something T2
    command C1
}
```

The following two figures show the difference of the two approaches.

<div align="center">
<img src="../pic/interactive_tx.png" width="20%">
&nbsp&nbsp&nbsp&nbsp
<img src="../pic/procedural_tx.png" width="26%">   
</div>

