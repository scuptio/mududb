# AssemblyScript Procedure Component 构建与 Compose TODO

## 目标

把当前 AssemblyScript transpiler 产出的 `.gen.ts`、`.gen.rs`、`.gen.wit`、`.desc.json` 接入完整构建链路，最终输出可被 MuduDB 运行的 WASI P2 component / package。

当前状态：

1. `mtp assembly-script` 已能发现 `/**mudu-proc*/` 并生成 adapter、Rust P2 wrapper、procedure WIT 和 procedure desc。
2. AssemblyScript binding 可通过 `npm run build` 编译。
3. 尚未把 AssemblyScript core wasm、Rust P2 wrapper component 和 compose 步骤串成自动流程。

---

## 期望构建流水线

```text
1. 扫描 AssemblyScript source
2. mtp assembly-script
   -> procedure.gen.ts
   -> procedure.gen.rs
   -> procedure.gen.wit
   -> procedure.desc.json
3. asc 编译 procedure.gen.ts
   -> procedure.as.wasm
4. 将 AssemblyScript core wasm componentize
   -> procedure.as.component.wasm
5. 编译 Rust P2 wrapper 到 wasm32-wasip2
   -> procedure.wrapper.component.wasm
6. wasm-tools compose
   -> procedure.component.wasm
7. mpk/package 阶段合并 desc
   -> package.desc.json / mpk
```

---

## TODO

### 1. 明确构建输入与目录布局

- [ ] 定义 AssemblyScript procedure 项目的推荐目录结构。
- [ ] 明确用户源码目录、生成目录、artifact 目录和最终 component 输出目录。
- [ ] 支持单文件输入和多文件项目输入。
- [ ] 决定 adapter 是追加到 `.gen.ts` 还是生成独立 `.adapter.ts`。

建议布局：

```text
src/
  assembly/
    procedure.ts
generated/
  assembly/
    procedure.gen.ts
    procedure.gen.rs
    procedure.gen.wit
artifact/
  as/
    procedure.as.wasm
    procedure.as.component.wasm
  rust/
    procedure.wrapper.component.wasm
  component/
    procedure.component.wasm
  desc/
    procedure.desc.json
```

### 2. 扩展构建配置

- [ ] 扩展 `build-cfg/transpiler-cfg.toml` 支持 `lang = "assemblyscript"`。
- [ ] `script/build/transpiler.py` 支持 `.ts` include pattern。
- [ ] 增加 AssemblyScript 专属配置段。

建议配置：

```toml
lang = "assemblyscript"

[patterns]
include = ["**/*.ts"]
exclude = ["**/*.test.ts", "**/node_modules/**", "**/build/**"]

[assemblyscript]
entry = "assembly/index.ts"
asc = "npx asc"
componentize = "wasm-tools component new"
compose = "wasm-tools compose"
```

### 3. 接入 mtp assembly-script

- [ ] `script/build/transpiler.py` 根据 `lang = "assemblyscript"` 调用 `mtp assembly-script`。
- [ ] 输出 `.gen.ts/.gen.rs/.gen.wit/.desc.json`。
- [ ] 每个源文件生成独立 desc 后复用现有 `mpk merge-desc`。
- [ ] 失败时输出源文件路径、procedure 名和 transpiler stderr。

### 4. 编译 AssemblyScript core wasm

- [ ] 调用 `asc` 编译生成的 `.gen.ts`。
- [ ] 明确 runtime 选项，例如 `--runtime stub` 或项目默认 runtime。
- [ ] 明确 import/export 策略，确保 adapter 函数被导出。
- [ ] 将 AssemblyScript 编译错误映射到源文件路径。

示例：

```bash
npx asc generated/assembly/procedure.gen.ts \
  --target release \
  --outFile artifact/as/procedure.as.wasm
```

### 5. Componentize AssemblyScript wasm

- [ ] 选择 componentize 工具。
- [ ] 明确 `procedure.gen.wit` 与 common shim WIT 的组合方式。
- [ ] 生成 AssemblyScript procedure component。
- [ ] 验证 component export 包含 `procedure-p.adapter-p`。

候选命令：

```bash
wasm-tools component new artifact/as/procedure.as.wasm \
  --adapt wasi_snapshot_preview1=... \
  -o artifact/as/procedure.as.component.wasm
```

具体参数需要根据 AssemblyScript 产物是否依赖 WASI 决定。

### 6. 编译 Rust P2 wrapper component

- [ ] 为生成的 `.gen.rs` 准备临时 Rust crate 或接入现有 package crate。
- [ ] 编译 target `wasm32-wasip2`。
- [ ] 确保 wrapper component import procedure 专属 WIT interface。
- [ ] 确保 wrapper component export `mp2_P` 对应的 Mudu procedure world。

示例：

```bash
cargo build --target wasm32-wasip2 --release
```

### 7. Compose Rust wrapper 与 AssemblyScript component

- [ ] 使用 `wasm-tools compose` 将 Rust wrapper import 连接到 AssemblyScript adapter export。
- [ ] 产物输出为最终 procedure component。
- [ ] 验证 compose 后 component 不再有未满足的 procedure adapter import。
- [ ] 保留 compose graph 或 diagnostics 方便排错。

示例：

```bash
wasm-tools compose \
  artifact/rust/procedure.wrapper.component.wasm \
  -d artifact/as/procedure.as.component.wasm \
  -o artifact/component/procedure.component.wasm
```

### 8. Package / MPK 集成

- [ ] 将最终 `procedure.component.wasm` 接入现有 package 构建。
- [ ] 合并 `.desc.json` 到 package desc。
- [ ] 确认 package cfg 中 module name 与 desc module name 一致。
- [ ] 生成可运行 `.mpk`。

### 9. 构建验证

- [ ] 对生成 `.gen.ts` 执行 `asc` 编译测试。
- [ ] 对生成 `.gen.rs` 执行 `cargo check/build`。
- [ ] 对生成 `.gen.wit` 执行 `wit-parser` 或 `wasm-tools component wit` 校验。
- [ ] 对 compose 产物执行 `wasm-tools validate`。
- [ ] 增加端到端测试：调用 MuduDB procedure，确认参数传入和返回值正确。

### 10. 错误诊断

- [ ] 每个阶段输出明确 stage 名称。
- [ ] 错误信息包含 source file、procedure name、generated artifact path。
- [ ] 保留中间产物，避免失败后无法复现。
- [ ] 对 compose/link error 给出常见原因提示，例如 WIT package/interface/function name 不匹配。

---

## 验收标准

1. 一个 AssemblyScript procedure 项目可以通过统一 build 命令完成 transpile、compile、componentize、compose 和 package。
2. 最终产物包含可运行的 WASI P2 component。
3. `.desc.json` 与 Rust wrapper 内部 desc 一致。
4. compose 后没有未解析的 procedure adapter import。
5. 至少一个端到端测试能调用 AssemblyScript procedure 并返回正确结果。
