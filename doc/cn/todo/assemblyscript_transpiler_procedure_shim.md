# AssemblyScript Procedure Transpiler 与通用 Procedure Shim 需求文档

## 1. 背景

MuduDB 当前的 Rust procedure 体系已经承担了数据库访问、参数编码、返回值解码、事务上下文和运行时调用等核心逻辑。为了支持 AssemblyScript、Go、Python 等更多语言编写数据库内过程，需要避免把 MuduDB 内部的 tuple layout、SQL 参数编码、结果解码和 runtime syscall 细节扩散到每一种语言绑定中。

本需求提出一种统一模式：

```text
Rust P2 wrapper procedure
  -> common language procedure.wit shim ABI
      -> AssemblyScript procedure adapter
          -> concrete AssemblyScript procedure P
```

也就是说，非 Rust 语言只处理通用 `value-list` 输入输出；复杂的 MuduDB encode/decode 与数据库运行时交互仍由 Rust P2 wrapper 承担。

---

## 2. 目标

1. **为 transpiler 增加 AssemblyScript procedure 支持**  
   transpiler 能识别 AssemblyScript 源码中的 procedure 标记，并生成必要的 AssemblyScript adapter 与 Rust P2 wrapper。

2. **不把 AssemblyScript procedure 编译成 async/await 形式**  
   AssemblyScript 用户过程保持同步函数风格，不要求业务代码显式使用 `async`、`await` 或 Promise 风格 API。

3. **支持注释导语标记 procedure**  
   采用类似 Rust 现有做法的注释标记，例如：

   ```ts
   /**mudu-proc*/
   export function transfer(id: Oid, values: ValueList): ValueList {
     ...
   }
   ```

   transpiler 通过该标记发现 AssemblyScript procedure。

4. **为 AssemblyScript procedure 生成 Rust P2 wrapper**  
   参考现有 Rust example 的生成代码，假设 AssemblyScript 过程名为 `P`，transpiler 生成 Rust 侧 P2 入口 `mp2_P(param: Vec<u8>) -> Vec<u8>`，以及内部桥接函数 `mudu_inner_p2_P(ProcedureParam) -> RS<ProcedureResult>`。`mp2_P` 负责接入现有 procedure invoke 编码入口，`mudu_inner_p2_P` 再通过 common language procedure shim ABI 直接调用 procedure 专属的 AssemblyScript adapter `adapter_P`。

5. **统一多语言接入模式**  
   后续接入 Go、Python 等语言时，也采用同一套路：Rust P2 wrapper 负责 MuduDB 类型和运行时适配，目标语言只实现 common procedure ABI 上的业务逻辑。

---

## 3. 非目标

1. **不要求 AssemblyScript 侧实现 MuduDB tuple encode/decode**  
   AssemblyScript 不直接处理 MuduDB 内部二进制布局、tuple descriptor、SQL 参数编码或结果集底层解码。

2. **不要求 AssemblyScript 过程直接访问 Rust 内部类型**  
   跨语言边界只通过 WIT ABI 中的 `oid`、`value`、`value-list` 和 `error` 表达。

3. **不要求第一阶段支持泛型或复杂静态类型映射**  
   初期可以只支持 `ValueList -> ValueList` 的通用调用模式。强类型 wrapper 可以由 Rust 侧生成。

4. **不要求 `asc` 直接产出完整 WASI P2 component**  
   AssemblyScript 可以先编译为 core wasm，再通过 adapter 和 component composition 工具生成最终 component。

---

## 4. Common Procedure ABI

transpiler 为每一个 AssemblyScript procedure 生成对应的 WIT interface 和 Rust adapter。不同 procedure 不共用一个 `invoke(name, ...)` dispatch 入口，而是让 Rust `mp2_P` 直接调用对应的 `adapter_P`。

假设 AssemblyScript procedure 名为 `P`，生成的 WIT 形态如下：

```wit
package mududb:component-shim;

interface procedure-p {
    use types.{error, oid};
    use system.{value-list};

    adapter-p: func(id: oid, values: borrow<value-list>) -> result<value-list, error>;
}
```

说明：

1. `procedure-p` 是按 procedure 名称生成的 interface；
2. `adapter-p` 是 Rust 调用 AssemblyScript procedure `P` 的专用 ABI 函数；
3. `id` 是 session / transaction / context 对应的 MuduDB object id；
4. `values` 是入参列表，使用 `borrow<value-list>`，调用方不转移所有权；
5. 返回值是新的 `value-list`，由被调用语言创建并返回；
6. 错误统一转换为 `error`。

生成独立 WIT 的原因：

1. Rust 侧 `mp2_P` 可以静态绑定到 `adapter_P`，不需要运行时字符串 dispatch；
2. 每个 procedure 的导出边界更清晰，便于 compose、调试和链接错误定位；
3. 后续支持 Go、Python 等语言时也可以保持相同模式：每个 procedure 生成自己的 adapter。

---

## 5. ValueList 能力要求

为了让 AssemblyScript 过程能读取输入并构造返回值，`value-list` 资源需要同时支持写入和读取。

最低需求：

```wit
resource value-list {
    constructor();
    bind-named-value: func(name: string, value: value);
    bind-value: func(index: s32, value: value);

    len: func() -> u32;
    value: func(index: u32) -> result<value, error>;
    value-by-name: func(name: string) -> result<value, error>;
}
```

可选需求：

1. `find-name(name: string) -> option<u32>`；
2. `name(index: u32) -> result<option<string>, error>`；
3. `push(value: value)`，用于更自然地构造返回列表；
4. 明确 indexed values 是 0-based 还是 1-based，并在所有语言绑定中保持一致。

---

## 6. Transpiler 需求

### 6.1 AssemblyScript procedure 发现

transpiler 扫描 AssemblyScript 源码，识别如下形式。AssemblyScript parser 可以基于 `tree-sitter-typescript` 实现，复用 Tree-sitter 的注释、函数声明、export 修饰符、参数列表和返回类型解析能力。

```ts
/**mudu-proc*/
export function P(id: Oid, values: ValueList): ValueList {
  ...
}
```

后续可扩展支持带参数的注释导语：

```ts
/**
 * mudu-proc
 * name: transfer
 * rust-name: transfer
 */
```

### 6.2 AssemblyScript adapter 生成

transpiler 为每个标记的 AssemblyScript procedure 生成对应 adapter，而不是生成统一 dispatch。

假设过程 `P` 的约定签名为：

```ts
/**mudu-proc*/
export function P(id: Oid, values: ValueList): ValueList;
```

则生成对应 adapter：

```ts
export function adapter_P(id: Oid, values: ValueList): ValueList {
  return P(id, values);
}
```

具体签名规则由 transpiler 配置统一约束。

### 6.3 Rust P2 wrapper 生成

假设 AssemblyScript procedure 为 `P`，生成代码应参考现有 Rust procedure example 中的形态，而不是另起一套强类型入口。

```rust
fn mp2_P(param: Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure(
        param,
        mudu_inner_p2_P,
    )
}

pub fn mudu_inner_p2_P(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let values = build_value_list_from_procedure_param(&param)?;
    let return_values = procedure_p::adapter_p(param.session_id(), &values)?;
    build_procedure_result(return_values)
}
```

Rust P2 wrapper 负责：

1. `ProcedureParam` 到 `value-list` 的转换；
2. 通过 procedure 专属 WIT 调用 AssemblyScript `adapter_P`；
3. `value-list` 返回值到 `ProcedureResult` 的转换；
4. MuduDB error 与 Rust error 的互转；
5. 生成 `mudu_argv_desc_P`、`mudu_result_desc_P`、`mudu_proc_desc_P` 等 descriptor 函数；
6. 保持与现有 Rust generated procedure 一致的导出入口和命名体验。

---

## 7. 构建需求

构建过程应拆成几个阶段：

```text
1. transpiler 扫描 Rust + AssemblyScript procedure
2. 为每个 AssemblyScript procedure 生成专属 WIT、Rust P2 wrapper 和 AssemblyScript adapter
3. Rust 编译为 wasm32-wasip2 component
4. AssemblyScript 编译为 core wasm
5. 将 AssemblyScript core wasm 适配为 WASI P2 component
6. compose Rust component 与 AssemblyScript component
7. 输出最终可运行的 WASI P2 component / package
```

要求：

1. Rust 与 AssemblyScript 构建产物使用同一套 WIT package；
2. 每个 AssemblyScript procedure 有对应的 WIT interface 和 adapter 函数；
3. 构建脚本应明确区分 core wasm 编译、component adapter、component composition；
4. 产物中 Rust component import procedure 专属 adapter interface，AssemblyScript component export 对应 adapter interface。

---

## 8. 风险与约束

1. **跨 component resource 生命周期**  
   `value-list` 是 resource，需要明确输入 borrow、返回 own 的所有权规则。

2. **重入问题**  
   AssemblyScript procedure 可能通过 `system.query/command` 回调 Rust。Rust 调用 AssemblyScript 时不能持有会导致重入死锁的数据库锁、session 锁或 `RefCell` borrow。

3. **多 procedure adapter 管理**  
   每个 procedure 都生成独立 WIT 和 adapter，需要保证命名稳定、避免 interface/function 名冲突，并让 compose 阶段能准确连接 Rust import 与 AssemblyScript export。

4. **AssemblyScript 类型能力有限**  
   第一阶段应避免复杂泛型、反射和自动类型推断，优先使用显式 `ValueList` API。

5. **调试链路变长**  
   Rust P2 wrapper -> WIT ABI -> AssemblyScript adapter -> concrete procedure 的调用链需要保留 procedure name、source location 和 error source，便于定位问题。

---

## 9. 验收标准

1. transpiler 能识别 `/**mudu-proc*/` 标记的 AssemblyScript procedure；
2. 能为每个 AssemblyScript procedure 生成对应 WIT interface 和 AssemblyScript `adapter_P`；
3. 能生成与现有 Rust example 一致的 `mp2_P(param: Vec<u8>) -> Vec<u8>` 和 `mudu_inner_p2_P(ProcedureParam) -> RS<ProcedureResult>`；
4. Rust P2 wrapper 调用能通过 procedure 专属 WIT adapter 进入 AssemblyScript procedure；
5. AssemblyScript procedure 能读取 `value-list` 入参并返回 `value-list`；
6. 构建流程能将 Rust 和 AssemblyScript 产物组合为 WASI P2 component；
7. 不要求 AssemblyScript 侧处理 MuduDB tuple encode/decode；
8. 同一模式可复用于 Go、Python 等其他语言。
