# Aeneas + Lean 形式化验证方案

*English version: [formal_verification.md](formal_verification.md)*

本文约定 MuduDB 引入 Aeneas + Lean 的项目结构、试点范围和维护流程。
第一阶段已实现生产长度前缀缓冲区提取、Lean 规格与证明，以及 Python 安装/验证脚本。
操作入口见 [验证 README](../../verification/README.zh-CN.md)。工具检查和无需 root 的安装
统一接入 `cargo make doctor` / `cargo make doctor -- install --only formal-verification`；
验证入口为 `cargo make verify`、`verify-extract`、`verify-self-test`。
采用固定并校验 SHA256 的官方工具包，避免要求 OPAM/OCaml 系统依赖。
后续模块的扩展仍按本文方案推进。

当前注册表还覆盖生产代码中的 RecordSlot 编解码与 fragment 核心、页布局检查算术、
WAL 恢复与 GC 纯决策，证明路径分别镜像 `record_slot_core.rs`、`layout_math.rs` 和
`layout/decision.rs`。公开 API 与磁盘格式保持兼容。具体前提及 Rust 回归测试边界见
verification README；这些定理不代表文件系统上的完整崩溃恢复已被形式化证明。

## 目标与边界

采用“生产 Rust 单一来源、独立 Lean 证明工程、CI 校验代码与证明同步”的方式，
优先验证安全、顺序执行、无 I/O 的算法核心。

```text
生产 Rust → Charon → LLBC → Aeneas → Lean 生成实现
                                      + 行为规格与证明
                                      → Lean 内核检查
```

Aeneas 负责翻译，功能正确性仍需要维护者定义规格并完成证明。Lean 检查的是形式命题，
因此必须同时说明生产代码到生成模型的关联、提取配置和外部依赖假设。

根据当前官方说明，unsafe 和并发仍属于 Aeneas 的支持限制。第一阶段不覆盖整个数据库，
也不以局部算法证明代替集成测试、模糊测试或内部确定性模拟。特别是：恢复算法在给定
持久化模型下正确，不等于真实文件系统上的断电恢复已经得到证明。

## 目录结构与命名

保留现有 Cargo workspace 分组。`verification/lean/` 下的生产代码映射目录必须与
仓库原路径一致，保留 `crates`、分组、crate、`src` 和模块层级；未来覆盖 `example/`
时也按原路径镜像。新增目录和文件采用项目已有的 `snake_case` 风格，已有名称
（例如 `db-kernel`）原样保留。

不在 Lean 根目录使用 `Generated/`、`Spec/`、`Proof/` 将同一个生产模块拆散。
生成实现、规格和证明放在对应模块目录内。

```text
verification/
  README.md
  toolchain.lock.toml
  targets.toml
  lean/
    lean-toolchain
    lakefile.toml
    lake-manifest.json
    crates/
      common/
        mudu/
          src/
            common/
              endian/
                generated.lean
                spec.lean
                proof.lean
            utils/
              buf/
                generated.lean
                spec.lean
                proof.lean
      db-kernel/
        storage/
          src/
            page/
              record_slot/
                generated.lean
                spec.lean
                proof.lean
        wal/
          src/
            worker_wal_backend/
              layout/
                gc/
                  generated.lean
                  spec.lean
                  proof.lean
    support/

script/verification/
  toolchain.py
  check.py
  test_verification.py

doc/dev/formal_verification.md
```

路径映射规则：

```text
crates/<group>/<crate>/src/<path>/<module>.rs
    ↓
verification/lean/crates/<group>/<crate>/src/<path>/<module>/
```

对于 Rust 的目录模块 `<module>/mod.rs`，使用对应的 `<module>/` 目录，子模块继续
向下镜像。crate 根 `lib.rs` 的专属证明放在对应的 `src/lib/`，避免与其他模块混用。
移动或重命名 Rust 模块时，同步更新证明目录与目标清单。

| 文件或目录 | 职责 |
| --- | --- |
| `generated.lean` | 生产实现的自动翻译，禁止手改 |
| `spec.lean` | 独立于实现的行为定义、前置条件、后置条件 |
| `proof.lean` | 生成实现满足规格的证明 |
| `external.lean` | 按需增加的外部依赖模型，必须记录关联假设 |
| `support/` | 跨模块共享的引理和证明辅助，不保存第二份业务实现 |
| `targets.toml` | Rust 入口、提取配置、Lean 定理、假设和覆盖状态的映射 |
| `toolchain.lock.toml` | 自定义版本清单，由验证脚本读取；不是工具自带的锁文件 |

`generated.lean` 是项目期望的逻辑入口，不假设 Aeneas 可以直接按上述粒度输出。
首次提取应确认其类型、函数和外部模板的实际拆分方式，再由脚本确定性地组织文件及
导入关系。若只能按 crate 生成共享声明，将其放到该 crate 镜像目录下的
`generated/`，模块入口引用它；不得复制定义或手工修改生成结果。

物理目录保留 `db-kernel` 等原名。Lean 的 import、模块标识和 namespace 如需转义，
在生成脚本中统一处理，并通过 Lake 构建验证；不得为迁就标识符而悄悄改掉镜像目录名。
工具要求的 `lean-toolchain` 等标准文件名保留原样。

## 生产代码组织原则

1. 生产 Rust 是唯一实现来源。优先直接提取目标函数及依赖，不维护验证专用算法副本。
2. 先做小规模提取实验，再根据实际障碍调整依赖。不得为了预想的工具限制大规模重构。
3. 先按模块隔离纯逻辑；确有必要时才拆 crate，并按领域命名，例如 `page_format`。
   生产代码必须调用被验证的实现，不能只验证一个未接入的替代版本。
4. 数据库专属算法继续位于 `db-kernel` 层，不因形式化验证需要迁入纯基础库 `mudu`。
5. 如需单独提取入口，应引用同一份生产代码，并记录 features、cfg、目标平台与依赖。
   任何验证配置与生产配置的差异都需要审查，不能通过 cfg 替换实际算法。
6. 新增 crate 遵循 `ARCHITECTURE.md`，更新架构检查并保持跨 workspace 依赖为 rlib-only。
   修改 workspace manifest 时考虑内部 DS overlay 基线更新，不引入内部模拟后端依赖。

## 第一阶段：长度前缀缓冲区

首选 `crates/common/mudu/src/utils/buf.rs` 的 `write_sized_buf` 和
`read_sized_buf`，连同必要的端序辅助函数建立最小闭环。它们代码短、无 I/O，且有
明确的输入输出关系，适合检验工具链和证明维护成本。

初次源码审阅发现以下边界。第一阶段已通过明确错误类型、长度检查及对应 Lean 证明处理：

- `src.len() as u32` 的可表示范围，以及长度加上 4 字节头的溢出行为。
- 目标缓冲区不足时返回什么，以及是否保持输出不变。
- 头部不足和 payload 不足的错误语义。
- 原 `read_sized_buf` 文档描述错误值为预期总长度，而实现返回 payload 长度 `n`；
  新接口统一使用 `BufferTooSmall.required` 表示总长度，详见验证 README。

第一批性质：

| 性质 | 目标契约 |
| --- | --- |
| 编解码往返 | 容量足够、长度可表示时，读取写入结果得到原 payload |
| 消耗长度 | 成功时消耗长度恰好为 `4 + payload.len()` |
| 写入范围 | 成功只修改目标区间，失败不修改输出 |
| 截断处理 | 头部不足、payload 不足分别返回约定错误 |
| 算术安全 | 明确的前提下没有溢出、越界或 panic |
| 成功与终止 | 合法输入下确实成功并终止，而非仅证明成功分支的结果 |

规格需要区分数学自然数与 Rust 固定位宽整数，说明 `usize` 的目标位宽。
不要通过过强前置条件排除本应由 API 处理的非法输入；对于这类输入，应证明其按约定
返回错误。若无法在现有返回类型下表达合理行为，先评审 API 调整。

第一阶段的验收范围为：明确上述契约、完成生产代码提取、证明约定性质、接入可重复
运行的 CI 检查，并登记剩余假设和未覆盖范围。

## 后续扩展顺序

| 阶段 | 对象 | 主要性质 |
| --- | --- | --- |
| 1 | buffer 与端序转换 | 往返、长度、边界、错误语义 |
| 2 | `storage/src/page/record_slot.rs` | 编解码往返、保留位、非目标字段不变 |
| 3 | 页空间分配与记录区间计算 | 不重叠、不越界、容量守恒 |
| 4 | WAL 纯恢复决策和 GC 边界计算 | 状态不变量、删除条件 |
| 后续 | 索引操作、事务状态机 | 组合正确性与更复杂的不变量 |

`RecordSlot` 已有往返与 fragment 标志测试，可作为第二阶段的规格参考，但
`byteorder`、错误宏等依赖仍需先检查提取支持情况。不要仅为消除证明义务，把这些
依赖声明为无条件正确的外部函数。

WAL 阶段应将纯决策与 I/O 执行分开，并显式列出持久化、调用顺序和执行成功等假设。
实际 I/O、并发和崩溃行为继续由集成测试、故障注入与内部 DS 验证。

## 工具链与生成物维护

- 固定 Aeneas commit、其匹配的 Charon commit、提取所需 Rust 工具链、Lean 版本及
  Lake 依赖。Lean 版本应匹配 Aeneas backend；不要分别追踪各工具的最新版本。
- 不改变普通开发的 Rust 工具链。提取构建显式使用独立目录，例如
  `target/verification/charon/`，并保留仓库已有 jobs 和测试并行限制。
- 提交生成的 Lean 文件，便于审查语义变化；LLBC、Lake 构建缓存等作为构建产物。
- 自动生成文件包含适合 Lean 的注释头，例如
  `-- Generated by script/verification/check.py using Aeneas. Do not edit manually.`。
- 生成脚本只清理其拥有的生成物，不能覆盖 `spec.lean`、`proof.lean` 和人工外部模型。
- 工具链升级单独提交，重新提取并运行全部登记证明，避免与业务逻辑变化混在一起。

官方基本命令链路如下；项目脚本还需显式设置工作目录、工具版本、提取范围和输出路径：

```text
charon cargo --preset=aeneas
aeneas -backend lean <input.llbc>
lake build
```

日常操作应使用 README 中的 cargo make 入口，由 Python 脚本固定配置。需要外部模型时，可使用官方
`-split-files` 机制生成缺失定义模板，再维护对应模型与假设。

## CI 与日常开发流程

`script/verification/check.py` 串起完整链路：

```text
校验工具链和提取配置
  → 从当前生产 Rust 重新生成 Lean
  → 检查生成物与提交版本一致（包括新增、删除文件）
  → 构建全部登记的规格和证明模块
  → 检查关键定理的公理依赖及覆盖清单
```

只运行 `lake build` 可能一直验证旧生成物；仅检查生成物也不能证明性质成立。
构建入口必须导入全部登记的证明模块，防止新增文件未被构建发现。验证脚本强制执行这一点：
前置阶段在 `lakefile.toml` roots 未覆盖 `targets.toml` 登记的全部模块时直接报错，
而不是等到稍后以缺少目标文件的错误形式暴露。

试点完成后，对已纳入验证范围的修改设置阻断门禁。增量触发必须包括目标函数的传递
依赖、Cargo 锁文件、features、提取脚本和工具链清单；定期运行全量验证补充检查。
Rust 的 fmt、clippy 和测试继续沿用当前仓库流程。

新增两个日常辅助：`cargo make verify-premises` 重跑前提 `established_by` 引用的测试，
被引用的测试若被改名、移动、忽略或失败则报错，使信任链的腐烂显性化；
`cargo make verify-spec-diff -- --diff <ref>` 对改动的 spec/proof Lean 文件输出声明级 diff
（新增/删除/签名变更），帮助评审聚焦后条件是否变弱——后者是咨询级设计：弱化与否由人判断，
辅助只负责聚焦。新目标从 `cargo make verify-new-target` 起步，它一步生成注册条目、
提取模块和带内嵌作者指南的 spec/proof 骨架。

修改已覆盖代码时，在同一 PR 中更新实现、规格、生成物和证明；审查规格变化是否
削弱了原有保证。重构若保持契约，应尽量维持规格稳定，只更新实现关联和证明。

## 证明质量与信任边界

- 规格描述可观察行为，不逐句复述实现。评审前置条件是否合理、是否可满足。
- 对关键定理检查 `#print axioms` 的依赖闭包，拒绝 `sorryAx` 和未经审查的项目公理。
  标准逻辑公理与外部模型假设应明确登记；仅搜索 `sorry` 不足以审计证明。
- AI 可以辅助写证明，但验收仍以工具链检查、规格评审及假设审计为准。
- 外部模型必须记录对应 Rust 定义、适用版本、语义依据和未证明的关联。
- Lean 内核检查不会自动证明 Rust 编译器、Charon、Aeneas 实现及外部模型全部正确；
  验证报告必须说明这些信任边界。
- 用“入口函数、提取配置、已证明性质、假设、未覆盖范围”衡量覆盖，不用证明行数
  或生成 Lean 的函数数量代替质量指标。

`targets.toml` 登记源路径与符号、传递依赖提取范围、features/cfg、目标平台、生成模块、
定理名、sanity 定理、假设、负责人、前提及其对应的 Rust 测试，以及状态。状态区分
“已提取”与“已证明”，翻译成功不能视为验证完成；已证明的目标必须至少携带一条经审计的
sanity 定理，表明规格在具体输入上确实成立。运行器校验整个注册表（字段模式、前提引用、
变异锚点唯一性），拒绝所有已提交 Lean 文件中的削弱证明构造（`sorry`、`admit`、项目
`axiom` 声明、`native_decide`、`implemented_by`），并支持按名称或按变更文件的增量目标
选择（`script/verification/impact.py`）；预提交钩子对暂存变更运行提取漂移门禁。

## 参考资料

- [项目架构](../../ARCHITECTURE.md)
- [Aeneas 官方仓库：安装、支持范围与外部模型](https://github.com/AeneasVerif/aeneas)
- [Aeneas Lean 教程](https://github.com/AeneasVerif/aeneas/tree/main/tests/lean/Tutorial)
- [Aeneas 证明工程概述](https://github.com/AeneasVerif/aeneas/blob/main/documentation/aeneas-overview.md)
- [Lean 编译流程与内核检查](https://lean-lang.org/doc/reference/latest/Elaboration-and-Compilation/)

工具支持范围会变化；正式落地时以锁定 commit 的文档和实际提取结果为准。

WAL P0 扩展直接提取生产追加预留与帧/前缀边界代码，证明追加游标保持、连续预留
不重叠、帧区间无溢出、前缀拒绝不可逆，以及组合后的接受字节数守恒。
校验结果与有效初始追加游标是明确前提；CRC、迁移和文件系统副作用不在证明范围内。
集成范围详见 `verification/README.zh-CN.md`。

P1 新增多帧计数与字节累积、截断与重置决策、有序序列编辑/页发布/载荷迁移，
以及按键页差量和重放门控。生产纯核心直接提取并带有明确的组合证明。
归并遍历、页格式集成、文件系统副作用和并发仍属于回归覆盖的集成边界；
前提与运行成本详见 `verification/README.zh-CN.md` 的 P1 章节。
