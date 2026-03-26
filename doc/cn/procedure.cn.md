# 交互式与过程式：应该如何选择？

交互式与过程式是构建数据库应用的两种不同方式。

## 交互式方式

采用交互式方式时，用户通过命令行工具、GUI 工具、客户端库或 ORM 框架直接执行 SQL 语句。

**优点**：

- **即时反馈**：可以立即看到执行结果。
- **快速原型开发**：适合探索、试验与调试。
- **工作流简单**：几乎不需要额外配置。
- **对初学者友好**：学习曲线较平缓。

**缺点**：

- **性能较差**：数据库客户端与服务器之间存在通信开销。
- **正确性挑战**：事务语义更容易出错。

## 过程式方式

采用过程式方式时，开发者使用存储过程、函数和触发器实现业务逻辑。

**优点**：

- **性能优化**：减少网络开销。
- **代码复用**：业务逻辑集中管理。
- **事务控制**：更容易满足 ACID 要求。
- **更高的安全性**：降低 SQL 注入风险。

**缺点**：

- **学习曲线陡峭**：需要掌握数据库特定的过程式语言。
- **调试困难**：排查问题更加复杂。
- **厂商锁定**：在不同 DBMS 之间的可移植性较差。
- **版本控制困难**：通常需要专门工具支持。

---

# Mudu Procedure（MP）：统一交互式与过程式执行

同一份代码既可以在交互式模式下运行，也可以在过程式模式下运行。

MP 的目标是结合这两种方式的优势，同时避免它们常见的缺点。你可以使用大多数现代编程语言来编写 Mudu Procedure，而不必依赖 PostgreSQL PL/pgSQL 或 MySQL 存储过程这类数据库特有的过程式语法。

在开发阶段，Mudu Procedure 的运行方式更接近基于 ORM 框架的交互式开发体验。

## 当前实现（Rust）

Mudu Runtime 当前支持 Rust。基于 Rust 的过程使用如下函数签名：

## 过程规范

``` 
#[mudu_proc]
fn {procedure_name}(
    xid: XID,
    {argument_list...}
) -> RS<{return_value_type}>
```

### {procedure_name}：

一个合法的 Rust 函数名。

### 宏 `#[mudu_proc]`：

用于将该函数标记为 Mudu Procedure。

### 参数：

#### xid：

事务 ID。

### {argument_list...}：

实现 `Entity` trait 的输入参数。

支持的类型：`i32`、`i64`、`String`、`f32`、`f64`。

不支持的类型：自定义结构体、枚举、数组或元组。

### 返回值：

#### {return_value_type}：

返回类型必须实现 [`Entity` trait](../lang.common/proc_key_traits.md)。支持的类型与参数类型相同。

结果类型别名 `RS` 定义如下：

```rust
use mudu::error::error::ER;
pub type RS<X> = Result<X, ER>;  // ER: Error
```

在过程内部，运行时可以调用[系统调用](syscall.cn.md)，例如 SQL 系统调用（`mudu_query` / `mudu_command`）或键值系统调用（`mudu_get` / `mudu_set` / `mudu_range`）。

## Mudu Procedure 中的 CRUD（Create/Read/Update/Delete）操作

Mudu Procedure 可以调用两个核心 API：

### 1. `query`

`query` 用于 `SELECT` 语句。

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

`query` 会自动执行 R2O（relation-to-object，关系到对象）映射，并返回一个由实现 `Entity` trait 的对象组成的结果集。

### 2. `command`

`command` 用于 `INSERT`、`UPDATE` 和 `DELETE` 语句。

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

### 两者通用参数

#### xid：

事务 ID。

#### sql：

使用 `?` 作为参数占位符的 SQL 语句。

#### params：

参数列表。


<!--
quote_begin
content="[KeyTrait](../lang.common/proc_key_traits.md#L-L)"
-->

## 关键 Traits

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

### Datum、DatumDyn

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

## 示例：钱包应用中的转账过程

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

## MP 与事务

Mudu Procedure 支持两种事务执行模式：

### 自动模式

每个过程都作为一个独立事务运行。该事务会：

- 在过程返回 `Ok` 时自动提交

- 在过程返回 `Err` 时自动回滚

### 手动模式

可以在多个 Mudu Procedure 之间传递事务 ID（`xid`），以实现显式事务控制。

#### 示例：

```
procedure1(xid);
procedure2(xid);
commit(xid); // Explicit commit
// or rollback(xid) for explicit rollback
```

# 使用 Mudu Procedure 的优势

## 1. 交互式与过程式共用同一套代码

“一次开发！”

Mudu Procedure 在交互式开发和生产部署中使用完全相同的代码。这消除了在不同工具之间来回切换的成本，并确保不同环境中的行为保持一致。

## 2. 原生 ORM 支持

无缝的对象关系映射。
框架通过 `Entity` trait 提供内置 ORM 能力。它可以自动将查询结果映射为 Rust 结构体，在保持类型安全的同时减少样板转换代码。

## 3. 对静态分析更友好

更利于验证 AI 生成的代码。

Mudu 的强类型 API 提供了：

1. 通过 `sql_stmt!` 宏进行 SQL 语法的编译期检查

2. 对参数和返回值进行类型校验

3. 更早发现 AI 生成代码中的错误，这对可靠性至关重要

## 4. 数据近源处理

显著提升处理效率。

可以直接在数据库内部执行数据转换。
一个例子是，在无需导出或导入数据的情况下准备 AI 训练数据集。

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

优势：对于大规模数据集，可以通过避免网络传输来获得更快的处理速度。

## 5. 扩展数据库能力

充分利用完整的编程语言生态。
当前你可以使用任意 Rust crate，未来也可以扩展到其他语言生态。

例如，你可以使用 `uuid` 和 `chrono` 这两个 crate：

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

优势：

1. 可以使用 UUID、时间日期、地理空间等专用库

2. 可以实现纯 SQL 难以完成甚至无法完成的复杂逻辑

3. 可以通过 Cargo、npm、pip 等熟悉的工具管理依赖

# 关键技术优势

| 特性 | 传统方式 | MP 优势 |
|:----------------|:---------------------------|:--------------------------|
| 开发与生产一致性 | CLI / 存储过程使用不同代码 | 使用相同代码库 |
| 类型安全 | SQL 错误在运行时暴露 | 编译期验证 |
| 数据移动 | 需要 ETL 流程 | 数据库内处理 |
| 可扩展性 | 依赖数据库专用扩展 | 可使用通用编程库 |

# MuduDB 如何统一对待交互式与过程式方式

与传统单体式数据库不同，MuduDB 被拆分为两个组件：Mudu Runtime 和 DB Kernel。

Kernel 提供核心基础能力、事务管理和存储能力。
Runtime 支持多语言生态。
它可以承载 VM（虚拟机），并执行由主流编程语言编译得到的 WASM 字节码模块。
在 MP 执行过程中，Runtime 会与 Kernel 协作完成整个过程。

为了说明这一点，考虑下面这个例子。
假设一个过程会执行查询 `Q1` 和 `Q2`、命令 `C1`，以及函数 `T1` 和 `T2`，其中 `T1` 与 `T2` 由高级语言编写并编译为字节码。

```
procedure {
    query Q1
    do something T1
    query Q2
    do something T2
    command C1
}
```

下图展示了这两种方式之间的差异。

<div align="center">
<img src="../pic/interactive_tx.png" width="20%">
&nbsp&nbsp&nbsp&nbsp
<img src="../pic/procedural_tx.png" width="26%">   
</div>
