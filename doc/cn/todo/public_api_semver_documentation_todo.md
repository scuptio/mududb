# 公共 API、SemVer 与文档 TODO

## 目标

明确哪些接口面向外部用户，并让兼容性和文档完整性成为发布门禁。

## TODO

- [ ] 将 workspace crate 分为公开、内部、工具、示例和测试五类。
- [ ] 为公开 crate 补齐 description、repository、documentation、rust-version 等 metadata。
- [ ] 在 workspace 中集中管理版本和通用 package metadata。
- [ ] 为公开 crate 增加 `cargo-semver-checks`。
- [ ] 分阶段启用 `#![warn(missing_docs)]`，清零后提升为 deny。
- [ ] 为公开类型、错误码、feature 和兼容保证编写 rustdoc 示例。
- [ ] 规定弃用周期、兼容 re-export 保留期限和移除条件。
- [ ] 维护人工编写的 CHANGELOG，区分破坏性变更、功能、修复和迁移步骤。
- [ ] 发布前验证文档示例和最小客户端示例能够编译运行。

## 验收标准

- [ ] 每个公开 crate 的稳定性等级和支持范围明确。
- [ ] CI 能发现非预期 SemVer 破坏。
- [ ] 公共 API 可从生成文档中独立理解和使用。

