# Crate 分层与依赖规则 TODO

## 目标

让 `ARCHITECTURE.md` 中的分层规则成为机器可执行的约束，降低核心模块之间的耦合。

## TODO

- [ ] 定义每一层允许依赖的 crate 集合，并形成可检查的依赖规则文件。
- [ ] 在 CI 中生成依赖图并拒绝逆向依赖、未批准跨层依赖和新的依赖环。
- [ ] 扩展 `cargo-deny`/自定义检查，不只约束 `mudu_sys_impl` 的 wrapper。
- [ ] 确保所有 workspace crate 使用 `[lints] workspace = true`。
- [ ] 将 `future_incompatible` 从 `allow` 提升为 `warn`，清零后改为 `deny`。
- [ ] 明确每个 crate 的 public API、internal API 和 feature 边界。
- [ ] 为新增 crate 编写职责说明、允许依赖和禁止依赖。

## 验收标准

- [ ] CI 能自动发现违反架构分层的依赖。
- [ ] 新增 crate 必须继承 workspace lint 并通过依赖规则检查。

