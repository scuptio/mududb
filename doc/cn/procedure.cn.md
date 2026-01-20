# 交互式 vs 过程式：选择哪种方式？

交互式与过程式代表了开发数据库应用的两种不同方法。

## 交互式方法

采用交互式方法时，用户通过命令行或GUI工具直接执行SQL语句，或使用客户端库及ORM映射框架。

**优势**：

- **即时反馈**：实时查看结果
- **快速原型设计**：适合探索和调试
- **简单工作流**：所需配置极少
- **新手友好**：学习曲线平缓

**劣势**：

- **性能低下**：DB客户端与服务器间的通信开销
- **正确性挑战**：易错的事务语义

## 过程式方法

过程式方法中，开发者使用存储过程、函数和触发器实现业务逻辑。

**优势**：

- **性能优化**：减少网络开销
- **代码复用**：业务逻辑集中化管理
- **事务控制**：更好的ACID合规性
- **增强安全性**：降低SQL注入风险

**劣势**：

- **陡峭的学习曲线**：需掌握特定数据库的过程语言
- **调试困难**：问题排查难度大
- **供应商锁定**：不同DBMS间可移植性有限
- **版本控制挑战**：需专用工具支持

# Mudu可移植数据访问(MPDA)代码：统一交互式与过程式执行

同一份代码可同时以交互式和过程式模式运行。

我们旨在融合两种模式的优点，同时消除其缺陷。MPDA实现了这一目标。您可使用大多数现代语言编写MPDA——无需依赖PostgreSQL
PL/pgSQL或MySQL存储过程等"怪异"语法。

开发过程中，MPDA如同ORM映射框架般以交互方式运行。

## 当前实现（Rust）

Mudu运行时目前支持Rust。基于Rust的存储过程采用以下函数签名：

### 过程规范

```
#[mudu_proc]
fn {procedure_name}(
    xid: XID,
    {argument_list...}
) -> RS<{return_value_type}>
```

### {procedure_name}:

有效的Rust函数名

### Macro #[mudu_proc]:

标识函数为MPDA的宏

### 参数:

#### xid:

事务ID

### {argument_list...}:

实现 `Entity` 特性的输入参数。

支持类型：`i32`, `i64`,  `String`, `f32`, `f64`。

不支持：自定义结构体、枚举、数组或元组。

### 返回值：

#### {return_value_type}:

实现 `Entity` 特性的返回类型（支持类型与参数相同）。

返回结果类型 `RS` 是 `Result` 枚举：

```rust
use mudu::error::error::ER;
pub type RS<X> = Result<X, ER>; // ER: 错误类型
```

## MPDA中的CRUD(Create/Read/Update/Delete)操作

MPDA可以调用2个API。

### 1. `query`

`query`SELECT语句
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

`query` 自动执行 R2O（关系对象映射），返回实现 `Entity` trait的对象结果集。

---

## `command`

用于 INSERT/UPDATE/DELETE 操作

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

### 通用参数：

#### xid:

事务 ID

#### sql:

使用 '?' 作为参数占位符的 SQL 语句

#### params:

参数列表

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

## MPDA的例子: 钱包应用转账过程

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

## Mudu 过程与事务

MPDA支持两种事务执行模式：

### 自动模式

每个过程作为独立事务运行：

- 过程返回 `Ok` 时自动提交
- 过程返回 `Err` 时自动回滚

### 手动模式

通过事务 ID (`xid`) 跨多个 Mudu 过程进行显式事务控制。

#### 示例:

```
procedure1(xid);
procedure2(xid);
commit(xid); // Explicit commit
// or rollback(xid) for explicit rollback
```

---

# 使用 Mudu 过程的优势

## 1. 单一代码库双模式支持

"一次开发，多处运行！"
Mudu 过程在交互式开发和生产部署中使用完全相同的代码，消除工具切换成本，确保环境一致性。

## 2. 原生 ORM 支持

无缝对象关系映射
框架通过 `Entity` 特征提供内置 ORM 能力，自动将查询结果映射到 Rust 结构体，在保持类型安全的同时消除样板代码。

## 3. 静态分析友好

AI 生成代码验证
Mudu 的强类型 API 支持：

1. 通过 `sql_stmt!` 宏在编译期检查 SQL 语法
2. 参数和返回值的类型验证
3. 对 AI 生成代码的早期错误检测（可靠性关键）

## 4. 近数据处理

显著提升效率。
直接在数据库中执行数据转换，例如无需导出/导入即可准备 AI 训练数据集。

```rust
// 准备AI训练数据，不必导入/导出  
#[mudu_proc]
fn prepare_training_data(xid: XID) -> RS<()> {
    mudu_command(xid, 
        sql_stmt!("..."),
        sql_param!(&[]))?;
    // Further processing...
}
```

优势：避免网络传输，海量数据集处理速度提升。

### 5. 扩展数据库能力

利用完整编程生态  
集成任意 Rust crate（或未来语言生态）：

示例，使用 `uuid` 和 `chrono` crate，

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

1. 使用库（UUID、日期时间、地理空间等）
2. 实现纯 SQL 无法完成的复杂逻辑
3. 通过 Cargo/npm/pip 管理依赖

# 核心技术优势对比传统模式

| 特性      | 传统方案         | MPDA优势 |
|:--------|:-------------|:-------|
| 开发生产一致性 | CLI/存储过程代码不同 | 统一代码库  |
| 类型安全    | 运行时 SQL 错误   | 编译期验证  |
| 数据移动    | 需要 ETL 管道    | 库内处理   |
| 扩展性     | 数据库特定扩展      | 通用编程库  |

--

# MuduDB如何统一处理交互式与过程式方法?

MuduDB与传统一体式架构数据库不同，它分为两个组件：
Mudu运行时和 DB内核。
内核提供基础语义、事务支持及存储能力。
运行时实现扩展功能支持与多语言生态兼容。
运行时运行一个虚拟机执行WASM间字节码模块，主流编程语言均可编译为此类字节码。
在执行Mudu内部过程时，运行时需与内核协同完成流程。

以下用例说明其运作机制：
假设某过程执行查询Q1/Q2、条件C1，以及高级语言实现的函数T1/T2（它们能被编译成字节码）。

```
procedure {
    query Q1
    do something T1
    query Q2
    do something T2
    command C1
}
```

下列两图展示了两种方法的差异。


<div align="center">
<img src="../pic/interactive_tx.png" width="20%">
&nbsp&nbsp&nbsp&nbsp
<img src="../pic/procedural_tx.png" width="26%">   
</div>

