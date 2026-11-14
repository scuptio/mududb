# 生产 Rust 形式化验证：安装、检查与排错

*English version: [README.md](README.md)*

验证注册表覆盖长度前缀缓冲区、RecordSlot 磁盘编解码与 fragment 更新、页空间计算、
WAL 的纯恢复与 GC 决策、MSSP 线协议编解码（MessagePack 原语与 16 字节帧头），以及
Raft 的纯决策核心（法定人数算术与投票谓词）。
提取入口直接引用生产源码，不维护替代算法，不增加项目公理。

## 存储与 WAL 验证范围

| 目标 | 保证 |
| --- | --- |
| `record_slot_core` | 32 字节小端布局、字段往返、保留数据、fragment 掩码、短缓冲区拒绝、输出尾部不变 |
| `layout_math` | 槽数组边界、记录区间、8 字节对齐、包含填充的容量守恒、与先前记录不重叠 |
| `decision` | 标识符后继不溢出、恢复不低于持久化边界、尾部复用、归档约束和严格删除条件 |

RecordSlot 公开 API 和磁盘格式保持不变。页重建先完成全部容量检查再修改页，空间不足时
保留原页；非法槽数量在形成切片前拒绝。WAL 标识符耗尽时返回错误，不回绕或复用；
非空日志目录中没有有效 frame 时也遵守持久化 GC 边界。GC 遇到保留或无法判断的 chunk
即停止，避免边界跨过保留文件；归档边界落后于已有 GC 边界时不继续删除。

WAL 定理以有效 frame 摘要和持久化边界为输入，不证明 CRC、目录枚举、文件系统持久化
顺序、并发 GC 或完整崩溃恢复。页分离定理要求先前区间结束于分配游标之前，不代表
任意已有页都不重叠。这些集成边界仍由 Rust 回归测试检查。

## 线协议编解码验证范围

| 目标 | 保证 |
| --- | --- |
| `mp_core` | 规范 MessagePack 原语：逐构造子解码-编码往返（在任意后缀输入上恰好消耗编码字节）、编码最小宽度 marker 选择、短输入与异类/保留 marker 拒绝、有符号/无符号链分工 |
| `mssp_frame` | 16 字节 MSSP 帧头布局：帧头字段在附加 body 上的往返（magic、版本、零 flags、消息种类）、短输入拒绝 |

字节级 MessagePack 编解码位于 `mudu_binding::universal::mp_core`，由 host 与独立 guest SDK
逐字共享（`mudu_api_sync` 镜像）。`Value` 分派、UTF-8 校验与浮点位转换留在 `mp_wire`
调用侧的证明边界之外，由与 `rmp_serde` 逐字节对比的测试和 golden corpus
（`testing/fixtures/golden/v1/`）作为前提钉死。完整的递归 `Value` 往返刻意不作声明：
逐构造子往返通过解码器的消耗字节计数组合；非负 `Int` 按规范解码回 `UInt`。

新增手写 Lean 文件使用英文注释解释契约、前提和关键步骤。目录保留生产路径中的
`db-kernel` 名称，Lean import 中写作 `«db-kernel»`。

## Raft 共识验证范围

| 目标 | 保证 |
| --- | --- |
| `raft_quorum` | 位置对齐的授权计数（排除 leader，等于规范计数且不超过选民数）、严格多数判定、完整降序列表上的高中位多数提交索引选取、法定人数相交性（同一选民列表上的两个严格多数必共享一个未被排除的授权选民） |
| `raft_vote` | 日志新鲜度的字典序 (term, index) 刻画、投票授予谓词的完整刻画、单任期单票安全性（已记录的投票在同一任期内拒绝其他候选人）、被授予的选票不会流向更旧的任期 |

Raft 目标覆盖提取到 `mudu_raft::core::quorum_core` 与 `mudu_raft::core::vote_core`
的纯决策核心。`HashMap` 授权收集、降序排序与 `RaftCore` 状态装配（`current_term`、
`voted_for` 持久化、日志头）留在 `quorum.rs`/`election.rs`，由 quorum 与选举测试
作为前提钉死。协议级活性与完整状态机安全性不在提取范围内，仍属于模拟测试
（以及潜在的 TLA+ 式模型检查）的领域。

## 环境与 prerequisite dependencies

首版支持 **Linux x86_64，包括 x86_64 WSL2**。请在 Linux 文件系统内运行 WSL2
构建。macOS、原生 Windows、ARM Linux 暂不提供自动安装支持。

需要 Python 3.12+（含 `tomllib` 与 tar 安全提取支持）、Git、Rustup、C 编译器、
GNU tar、zstd、curl 和 CA certificates。预留至少 20 GiB 磁盘；首次下载包含 Lean
编译器、Rust 编译器组件、Mathlib 源码与证明缓存，耗时取决于网络。构建默认最多使用
四个 Rust jobs 和四个 Lean 线程。

Ubuntu / Debian 的系统依赖由用户安装（doctor 不执行这些命令）：

```bash
sudo apt update
sudo apt install python3 git build-essential tar zstd curl ca-certificates
```

若尚无 Rustup，按 [Rustup 官方说明](https://rustup.rs/) 安装到用户目录：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup-init.sh
sh /tmp/rustup-init.sh -y --profile minimal
. "$HOME/.cargo/env"
cargo install cargo-make --locked
```

已有 Rustup / cargo-make 时跳过相应步骤。无需手工安装 OPAM、OCaml、Nix 或 Elan：
本项目使用校验 SHA256 的官方 Aeneas 发布包（含匹配的 Charon 和 Lean 后端）以及
官方 Lean 编译器包，直接调用其绝对路径。与最初源码构建设想相比，这减少了系统级依赖。

版本固定在 [toolchain.lock.toml](toolchain.lock.toml)：Aeneas
`nightly-2026.10.24-74aabd5`、配套 Charon、Rust `nightly-2026-09-17`、Lean `4.35.0`。
Mathlib 等依赖固定在 [Lake manifest](lean/lake-manifest.json)。不追踪浮动最新版本。

## 首次安装与验证

在仓库根目录运行：

```bash
cargo make doctor
cargo make doctor -- install --only formal-verification --dry-run
cargo make doctor -- install --only formal-verification
cargo make verify
```

`cargo make doctor` 只报告，不安装。`install` 下载并校验工具包、安装固定 Rust
组件和 Lean 依赖缓存；全程不使用 sudo，不修改默认 Rust 工具链或 shell 配置。
缺少系统依赖时会停止并提示。安装命令可重复执行，匹配的已安装项目会跳过。
所有步骤由 Python 脚本编排，不依赖额外 shell 安装脚本。

若尚未安装 cargo-make，也可以直接执行等价 Python 命令：

```bash
python3 script/doctor.py install --only formal-verification
python3 script/verification/check.py check
```

成功输出：

```text
PASS: production source extraction, Lean proofs and axiom audit
```

`verify` 从当前源码重新提取到临时目录，比对提交的生成文件，构建 Lean 证明，
最后检查全部登记定理的公理依赖。缺少工具不会被默默跳过，也不会在验证中自动安装。
不要仅凭直接运行 `lake build` 成功就认定生产代码已验证：它可能使用旧的生成实现。

## 代码、规格与目录

```text
crates/common/mudu/src/utils/buf.rs                 # 唯一生产实现
verification/extract/                             # 独立 Cargo 提取入口
verification/lean/crates/common/mudu/src/utils/buf/
  generated.lean                                  # 自动生成，禁止手改
  spec.lean                                       # 数学规格
  proof.lean                                      # 证明
script/verification/                              # Python 安装/验证/测试脚本
```

提取入口使用 `#[path]` 引用同一份生产模块，无 features 或验证专用 cfg。
Charon 以各目标登记的函数为入口，生成实现依赖 Aeneas 标准库模型；没有新增项目外部模型。
物理目录与生产代码一致。当前只涉及 `common`，后续 `db-kernel` 等目录也保留原名。

[targets.toml](targets.toml) 登记入口、平台、全部审计定理和允许的标准逻辑公理。

| 定理 | 保证 |
| --- | --- |
| `length_correct` | 总长度为 payload 长度加四，或准确报告溢出 |
| `write_success` | 输出为大端头、原 payload、原输出缓冲区的剩余尾部 |
| `write_overflow`、`write_short` | 失败类型准确，输出完全不变 |
| `read_short_header` | 不足四字节时报告可用长度 |
| `read_overflow` | 头部声明不可表示的总长度时拒绝 |
| `read_truncated` | payload 截断时报告所需总长度 |
| `read_success` | 返回准确总长度与输入中的对应 payload |
| `decode_header_append` | 数学规格的大端头编码和解码互逆 |
| `roundtrip` | 写入再读取还原原 payload，读写总长度一致 |

Aeneas 的成功后置条件同时要求函数返回成功的模型结果，而不是仅描述“如果成功”。
Rust API 的 `Err` 是正常返回值，与模型中 panic、越界、溢出或发散不同。

## API 迁移与兼容性

原来写入返回 `0` 表示错误，现在返回 `Result<u32, SizedBufError>`。
读取的错误从 `Option<u32>` 改为 `SizedBufError`：

- `LengthOverflow`：payload 加四不能用 `u32` 表示。
- `HeaderTooShort { available }`：不足四字节。
- `BufferTooSmall { required, available }`：容量不足，`required` 是包含头部的总长度。

成功长度继续使用 `u32`，最大 payload 为 `u32::MAX - 4`。先检查头部是否存在（读取），
再检查总长度可表示性，最后检查容量。写入任一错误均不修改输出。
`sized_buf_len` 供调用方在分配前检查长度。字符串编码调用方已迁移；有效输入的大端
二进制格式不变。这是 Rust 源码 API 的破坏性变更，仓库外调用方需处理新返回类型。

## 日常修改与验证

```bash
# 修改生产代码或规格后：
cargo make verify-extract
# 审查生成差异，更新规格/证明；不要编辑 generated.lean。
git diff -- verification/lean
cargo make verify

# Python 门禁测试：
python3 -m unittest discover -s script/verification -p 'test_*.py'

# Rust 检查继续使用仓库原有流程：
cargo make check
```

`verify-extract` 只更新生成实现，不保证证明成立。规格应表达预期行为，不能为了让错误
实现通过而削弱前提或结果。工具链升级单独提交，更新校验值和配套版本，再重新提取、
证明、自检。

## 注册表字段、sanity 检查与增量运行

[targets.toml](targets.toml) 中的每个目标现在都带有治理元数据，运行器在任何实际
工作之前先校验：

| 字段 | 规则 |
| --- | --- |
| `status` | `proven`（定理与 sanity 均审计）或 `extracted`（仅漂移检查）；翻译成功不等于证明完成 |
| `owner` | 非空的负责领域 |
| `assumptions` | 信任边界内、未机器检查的假设；即使为空也必须显式列出 |
| `sanity` | 证明规格在具体输入上确实成立的定理；`status = "proven"` 时至少一条，与主定理同样接受公理审计 |
| `premises` | 证明前提中由集成层建立的部分，每条用 `established_by` 以 `path` 或 `path::symbol` 形式指向 discharge 它的 Rust 测试或断言；引用不可解析则运行失败 |

同一预检还会在登记的 `mutation_from` 不再恰好匹配一次时快速失败，并拒绝所有已提交
`.lean` 文件中的削弱证明构造（`sorry`、`admit`、`axiom` 声明、`native_decide`、
`implemented_by`）——扫描前先剥离注释，所以文档化该禁令仍然允许。

sanity 定理位于每个 `proof.lean` 末尾，以 `sanity_*` 命名，把该目标已证明的契约实例化
到具体见证值（例如 `sanity_roundtrip` 实际写入并读回一个具体载荷）。它们堵住
"规格被削弱到空虚"的漏洞：任何具体输入都无法满足的规格将无法编译。

所有需要工具链的 action 都支持目标选择：

```bash
python3 script/verification/check.py check --targets buf,decision
python3 script/verification/check.py check --diff main          # 只验证 diff 影响的目标
python3 script/verification/check.py drift --staged             # 预提交钩子使用的形式
```

目标所属 crate 内的变更影响该 crate 的目标；`crates/common/` 下或验证机制自身的变更
影响全部目标（映射见 `script/verification/impact.py`，它同时校验 CI 工作流的路径
触发器覆盖每个已登记的 target）。

更多入口：

```bash
cargo make verify-drift            # 只提取 + 比对，不构建证明
cargo make verify-determinism      # 提取两次，要求生成 Lean 字节一致
cargo make verify-report           # 生成信任边界报告到 target/verification/report/
python3 script/verification/check.py mutation-sweep --list-only   # 只列出生成的候选变异
cargo make verify-mutation-sweep   # 运行生成的变异并报告击杀率（较慢）
cargo make verify-premises         # 运行前提证据引用的测试
cargo make verify-spec-diff -- --diff main   # spec/proof 文件的咨询级声明 diff
cargo make verify-new-target -- --name ... --source ... --owner ... --functions ... [--dry-run]
```

`verify-report` 不需要形式化工具链：先校验注册表，再写出 `verification-report.md` /
`.json`，包含每个目标的 status、owner、假设、前提及其对应测试，以及标准信任边界说明。

变异扫描为每个符合条件的位置生成一个单位置运算符翻转（`<`/`<=`、`>`/`>=`、`==`/`!=`、
`&&`/`||`），跳过已登记变异覆盖的位置和 `->`/`>>` 等 Rust 语法。每个候选被分类为：
被证明击杀、编译期击杀、silent（提取结果不变）或存活；存活者提示规格盲区。完整扫描
较慢（每个候选一次提取加一次证明构建），用 `--budget` 限制规模，手工或夜间运行——
CI 只列出候选作为冒烟检查。

`verify-premises` 运行 `established_by` 前提证据引用的测试（把 `crates/<group>/<crate>/src|tests/...`
路径映射为 cargo 调用），当被引用的测试被改名、移动、忽略或失败时报错——注册表的存在性
检查看不到这类腐烂。

`verify-spec-diff` 是咨询级的 spec 弱化审查辅助：把改动的 `spec.lean`/`proof.lean` 文件与基线
做声明签名级 diff，并在 CI 打印 `::warning`。后条件是否变弱仍是人工评审判断，该辅助只做聚焦。

## 新增验证目标的作者指南

`cargo make verify-new-target` 一步完成注册脚手架：提取用的 `#[path]` 模块行、`targets.toml`
条目（起始为 `status = "extracted"`）、`spec.lean`/`proof.lean` 骨架和 lakefile roots，随后提取
并编译骨架。生成的 `proof.lean` 内嵌作者指南（提炼自编解码证明实践），要点：

- 证明边界上算术（`* 256 +`）优于位打包；常量的 `>>` 和 `%` 保持在值层面可证
- if 链优于标量字面量 match
- 定长展开读取优于 `from_be_bytes` 数组
- 编码器用 `&mut Vec<u8>` push/extend_from_slice；解码器用
  `&[u8] -> Result<(T, usize), Error>` 并返回消耗字节数
- 证明骨架：`step`/`step with` 穿过 WP monad，容量侧条件交给 `scalar_tac`，切片索引用
  `getElem!_pos` 转换，marker 常量用 `simp only […MARKER_X]` 展开，sanity 用 `WP.spec_mono`

脚手架前，生产源码必须零依赖（提取工作区没有依赖）并采用上述形态；不满足就先重构
（编解码验证对 `mp_core`/`mssp_frame` 正是这样做的）。

## 故意引入错误：验证是否真的有效

```bash
cargo make verify-self-test
```

自检先要求正常源码通过完整验证，然后在 `target/verification/` 的临时副本中将写入
长度头改为零。它检查重新提取产生差异，再用错误生成实现构建原证明。错误实现和规格
必须先编译成功；只有证明文件报错才算检测成功。原始源码与提交的生成文件不被修改。

预期最后输出：

```text
PASS: every independent source mutation rejected by its production proof
```

这验证了源码关联与证明门禁两层。形式化证明失败通常显示未完成的目标或类型错误，
不会保证自动生成可执行反例；可结合 Rust 单元测试定位具体输入。

## 排错与日志

每次 `verify`、`verify-extract`、`verify-self-test` 运行均创建独立目录：
`target/verification/runs/<UTC时间>-<动作>-<随机标识>/`。开头和结束都会打印路径，
不同运行及正常/错误实现的日志不会互相覆盖。安装日志仍位于 `target/verification/logs/`。

输出保留完整命令、工作目录及日志路径，同时增加：

- 阶段编号、RUNNING / PASS / FAIL 状态和每阶段耗时；阶段总数随登记目标和自检变异数量伸缩。
- 每十秒一次的等待提示，显示正在执行的阶段与已用时间。
- 最终目标、成功阶段数、验证函数数、审计定理数及总耗时；提取命令明确说明未检查证明。
- 首个编译错误及其上下文、文件行列信息、原有最后 25 行日志和处理建议。
- 自检分别报告错误实现编译通过和证明按预期拒绝；基础设施错误仍作为失败处理。

`output.log` 保存阶段与命令摘要，`summary.json` 保存状态、版本、各阶段耗时和错误。
`extract-charon.log`、`extract-aeneas.log`、`lean-build.log`、`axiom-audit.log` 保存完整工具输出。
同名命令再次执行会使用数字后缀。自检另存 `mutant-compile.log`、`mutant-proof.log` 和
`mutant-sources/` 源码快照，便于临时目录清理后继续查看诊断。
生成文件不同步时，新的生成实现保留为 `fresh-generated.lean`。

终端输出复用项目颜色风格；重定向、CI、`NO_COLOR` 或 `TERM=dumb` 环境使用纯文本。
阶段进度按实际完成情况更新，不估算剩余时间。

| 失败位置 | 含义与处理 |
| --- | --- |
| doctor 缺少系统依赖 | 按上面的 prerequisite 步骤安装，重新运行 doctor |
| 下载失败 | 检查 GitHub、Rust 分发站点、Mathlib 缓存网络；重新执行 doctor install |
| SHA256 不符 | 拒绝安装；检查代理或下载损坏，不要直接修改预期校验值 |
| 工具缺失或版本漂移 | 运行 `cargo make doctor -- install --only formal-verification` |
| `extract-charon.log` | Rust 编译、提取入口、语言支持或固定工具链问题 |
| `extract-aeneas.log` | Aeneas 翻译失败；不能用 opaque 声明或公理绕过 |
| `Generated Lean is stale` | 当前 Rust 与生成文件不同；重新提取并审查 |
| `lean-build.log` | 定理或规格不能匹配生成实现；检查源码差异和未完成目标 |
| `axiom-audit.log` | 只允许 `propext`、`Classical.choice`、`Quot.sound`；其他依赖导致失败 |
| 自检错误实现通过 | 门禁失效或规格被削弱；不能作为正常成功忽略 |

工具、下载包和临时提取位于 `target/verification/`，Lean 包及编译缓存位于
`verification/lean/.lake/`。删除这些缓存后需要重新安装/构建；不要删除提交的 Lean
源文件、工具版本清单和 Lake manifest。系统 Rustup 管理的固定工具链留在用户目录。

## 信任边界与 CI

证明针对固定的 Linux x86_64 提取配置，不能据此宣称整个数据库已验证。Lean 内核、
Rust 编译器、Charon/Aeneas 翻译实现及 Aeneas 标准库模型属于信任边界；证明未覆盖
内存分配失败、操作系统、并发、持久化或磁盘故障。仍需运行 Rust 测试、模糊测试和集成测试。

独立 CI 工作流复用 doctor 和相同的 Python 验证脚本，执行验证、自检、提取确定性检查和
脚本测试，上传失败日志。PR 只验证 diff 影响的目标（由 `impact.py` 计算）；push、夜间
调度和手动运行始终验证完整注册表。另有独立步骤校验工作流路径触发器覆盖每个已登记的
目标源文件。预提交钩子在已安装形式化工具链时对暂存变更运行漂移门禁，未安装则静默跳过。
普通 `cargo make build` 和预提交钩子不隐式安装形式化工具。

变异自测对每个登记的错误分别创建隔离副本，先确认生成实现和规格可以编译，再要求
对应目标的原证明拒绝错误。一个目标的失败不能替另一个目标通过自测。

## P0：追加预留与帧前缀边界

新增生产函数 `append_plan::reserve`、`frame_bounds::bounds` 和
`Prefix::advance` 的直接提取与证明：

- 追加预留保持游标不变量；普通写入不超出 chunk；超大写入独占 chunk；
  新序号不回绕，零长度预留不改状态，序号耗尽时调用方不发布任何新状态。
- 连续两次成功的非空预留，其 chunk 序号递增，或在同一 chunk 中占用互不重叠的区间。
- 帧长度包含 24 字节头部与 8 字节尾部，计算不溢出；迁移后重新检查长度再切片。
- 前缀游标每次恰好前进一个被接受帧的长度，拒绝后不可恢复。
  对提取步骤的组合证明保证保留长度等于最大可接受前缀的长度之和。

追加证明要求初始游标有效：当前序号小于下一序号，当前大小不超过容量，
无当前 chunk 时大小为零。前缀组合证明以校验结果为输入，不证明 CRC 算法、
版本迁移或整个 Rust 解析循环；回归测试另对真实多帧数据逐字节截断，检查保留结果。
这些纯证明不代表文件系统持久性证明。新增目标同样纳入独立变异自测与公理审计。

## P1：WAL 与页操作的组合验证

| 生产目标 | 已证明的契约 |
| --- | --- |
| `part_chain` | 续片计数严格递减，仅零计数完成记录；追加字节不丢失、不重复；两次分批投递的字节结果可组合 |
| `truncate_plan` | 截断包含边界帧，仅删除后续 chunk；重置保留偏移且分配可表示的新序号；组合决策定位首个匹配帧的起点 |
| `sequence_edit` | 序列编辑精确保留前缀、替换内容和后缀；在排序及边界前提下保持有序；发布全成或全败；载荷迁移不改变目标范围之外的字节 |
| `delta_value` | 按键 diff 后 apply 等于新值；其他键不变；逐键提升得到逻辑页往返；LSN 门控下重复重放幂等 |

生产页修改现在使用栈上的 4 KiB 临时页，所有可失败步骤成功后才发布，页外后缀
保持不变。这增加了每次修改的页复制成本，不声称提高吞吐量。重建的范围与互斥
条件复用已有空间分配证明。泛型序列证明明确要求 Clone 保持值不变；有序编辑
要求保留部分、替换部分和跨边界比较满足排序关系。

逻辑页差量模型要求键唯一、载荷比较正确。生产归并遍历、排序库、页头/槽位集成
和文件操作仍由回归测试覆盖，并非整个包装函数每行代码的形式化证明。
截断组合以已校验的帧长度与 LSN 为输入；LSN 单调时，保留前缀才对应所有低于
边界的记录。实时截断仍要求没有并发 commit/promote。重复重放幂等性以成功构造
的页被标记为批次 LSN 为前提，不证明文件系统崩溃原子性或 fsync 顺序。

回归覆盖所有多帧投递切分点、错序拒绝后重试、实时截断/追加/重开、对照记录映射
的混合页编辑、失败时页字节不变，以及包含增删改与页链接的差量重建。
所有新增目标均加入独立变异自测。
