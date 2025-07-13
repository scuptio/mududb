# Interactive vs. Procedural: Which Is Your Choice?

Interactive and procedural approaches represent two distinct methods for developing database applications.
Interactive Development

When using the interactive approach, users directly execute SQL statements via command-line or GUI tools, or utilize client libraries or ORM mapping frameworks.

**Advantages**:

- **Immediate feedback**: View results instantly.

- **Rapid prototyping**: Ideal for exploration and debugging.

- **Simple workflow**: Minimal setup required.

- **Beginner-friendly**: Gentle learning curve.

**Disadvantages**:

- **Poor performance**: Communication overhead between DB client and server.

- **Correctness challenges**: Vulnerable transaction semantics.


Procedural Development

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

# Mudu Procedure: Unified Interactive and Procedural Execution

One piece of code can run both interactively and procedurally.

We aim to combine the advantages of both modes while eliminating their drawbacks. Mudu Procedure achieves this. You can write Mudu Procedures in most modern languages—without relying on "weird" or "ugly" syntax like PostgreSQL PL/pgSQL or MySQL’s stored procedures.

During development, Mudu Procedures run interactively like an ORM mapping framework.

## Current Implementation (Rust)

Mudu Runtime currently supports Rust. A Rust-based stored procedure uses the following function signature:


## Procedure specification
```rust
#[mudu_procedure]
fn {procedure_name}(
    xid: XID,
    {argument_list...}
) -> RS<{return_value_type}>
```
### {procedure_name}: 

Valid Rust function name.

### Macro #[mudu_procedure]: 

Macro identifying the function as a Mudu procedure.

### Parameters:

#### xid: 
Transaction ID.

### {argument_list...}: 

Input arguments implementing the `ToDatum` trait.

Supported types: `bool`, `i32`, `i64`, `i128`, `String`, `f32`, `f64`.

Unsupported: Custom structs, enums, arrays, or tuples.

### Return value:

#### {return_value_type}: 
Return type implementing the `ToDatum` trait (same supported types as arguments).

Return Result Type `RS` is `Result` enum:
```rust
use mudu::common::error::ER;
pub type RS<X> = Result<X, ER>; enum // ER: Error
```

## CRUD(Create/Read/Update/Delete) Operations in Mudu Procedures


There are two key APIs that a Mudu procedure can invoke:

### 1. `query`

`query` for SELECT statements


```rust
pub fn query<R: Record>(
    xid: XID,
    sql: &(dyn SQLStmt + Sync),
    params: &[&(dyn ToDatum + Sync)]
) -> RS<RecordSet<R>> { ... }
```

`query` Performs R2O(relation to object) mapping automatically, returning a result set of objects implementing the `Record` trait.

### 2. `command`

`command` for INSERT/UPDATE/DELETE

```rust
pub fn command(
    xid: XID,
    sql: &(dyn SQLStmt + Sync),
    params: &[&(dyn ToDatum + Sync)]
) -> RS<usize> { ... } // Returns affected row count
```

### Parameters for Both:

#### xid: 
Transaction ID.

#### sql: 

SQL statement with ? as parameter placeholders.

#### params: 

Parameter list.

## Key Traits

### SQLStmt

```rust

pub trait SQLStmt: std::fmt::Debug + std::fmt::Display {
    fn to_sql_string(&self) -> String;
}
```

### ToDatum
```rust

pub trait ToDatum: std::fmt::Debug {
    fn to_type_id(&self) -> DatTypeID;
    fn to_typed(&self, param: &ParamObj) -> RS<DatTyped>;
    fn to_binary(&self, param: &ParamObj) -> RS<DatBinary>;
    fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable>;
    fn to_internal(&self, param: &ParamObj) -> RS<DatInternal>;
}
```

### Record

```rust
pub trait Record: Sized {
    fn table_name() -> &'static str;
    fn from_tuple<T: AsRef<TupleRow>, D: AsRef<RowDesc>>(tuple_row: T, row_desc: D) -> RS<Self>;
    fn to_tuple<D: AsRef<RowDesc>>(&self, row_desc: D) -> RS<TupleRow>;
    fn get(&self, field_name: &str) -> RS<Option<Datum>>;
    fn set<D: AsRef<Datum>>(&mut self, field_name: &str, datum: Option<D>) -> RS<()>;
}
```

## A Example: A Wallet APP's Transfer Procedure
```rust

use mudu::{sql_param, sql_stmt, XID, RS, ER::MuduError};
use crate::rust::wallets::object::Wallets;
use uuid::Uuid;

#[mudu_procedure]
pub fn transfer_funds(
    xid: XID, 
    from_user_id: i32, 
    to_user_id: i32, 
    amount: i32
) -> RS<()> {
    // Validate amount
    if amount <= 0 {
        return Err(MuduError("Transfer amount must be > 0".into()));
    }
    if from_user_id == to_user_id {
        return Err(MuduError("Cannot transfer to self".into()));
    }

    // Check sender balance
    let mut wallet_rs = query::<Wallets>(
        xid,
        sql_stmt!("SELECT user_id, balance FROM wallets WHERE user_id = ?;"),
        sql_param!(&[&from_user_id]),
    )?;
    let from_wallet = wallet_rs.next()?
        .ok_or_else(|| MuduError("Sender not found".into()))?;
    
    if from_wallet.balance() < amount {
        return Err(MuduError("Insufficient funds".into()));
    }

    // Verify receiver exists
    let mut to_wallet_rs = query::<Wallets>(
        xid,
        sql_stmt!("SELECT user_id FROM wallets WHERE user_id = ?;"),
        sql_param!(&[&to_user_id]),
    )?;
    if to_wallet_rs.next()?.is_none() {
        return Err(MuduError("Receiver not found".into()));
    }

    // Execute transfer
    command(
        xid,
        sql_stmt!("UPDATE wallets SET balance = balance - ? WHERE user_id = ?;"),
        sql_param!(&[&amount, &from_user_id]),
    )?;
    
    command(
        xid,
        sql_stmt!("UPDATE wallets SET balance = balance + ? WHERE user_id = ?;"),
        sql_param!(&[&amount, &to_user_id]),
    )?;

    // Record transaction
    let trans_id = Uuid::new_v4().to_string();
    command(
        xid,
        sql_stmt!(
            "INSERT INTO transactions (trans_id, from_user, to_user, amount) 
             VALUES (?, ?, ?, ?);"
        ),
        sql_param!(&[&trans_id, &from_user_id, &to_user_id, &amount]),
    )?;

    Ok(())
}
```

## Mudu Procedure and Transaction
Mududb supports 2 transaction execution modes:

### Automatic Mode

Each procedure runs as an independent transaction. The transaction:

Commits automatically if the procedure returns Ok

Rolls back automatically if the procedure returns Err

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

Mudu Procedures use the exact same code for both interactive development and production deployment. This eliminates context switching between tools and ensures consistency across environments.

## 2. Native ORM Support

Seamless object-relational mapping
The framework provides built-in ORM capabilities through the Record trait. It automatically maps query results to Rust structs, eliminating boilerplate conversion code while maintaining type safety.

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
#[mudu_procedure]
fn prepare_training_data(xid: XID) -> RS<()> {
    command(xid, 
        sql_stmt!("..."),
        &[])?;
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

#[mudu_procedure]
fn create_order(xid: XID, user_id: i32) -> RS<String> {
    // Do something ....

    let order_id = Uuid::new_v4().to_string();
    let created_at = Utc::now().naive_utc();
    
    command(xid,
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


| Feature          | Traditional Approach       | Mudu Procedure Advantage               |
| :--------------- | :------------------------- | :--------------------------- |
| Dev-Prod Parity  | Different code for CLI/SPs | Identical codebase           |
| Type Safety      | Runtime SQL errors         | Compile-time validation      |
| Data Movement    | ETL pipelines required     | In-database processing       |
| Extensibility    | DB-specific extensions     | General-purpose libraries    |
