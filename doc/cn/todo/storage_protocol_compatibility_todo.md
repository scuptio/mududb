# 存储、协议与制品兼容性 TODO

## 目标

把落盘格式和外部协议视为长期契约，确保升级可验证、损坏可诊断、必要时可回滚。

## TODO

- [x] 盘点 file、page、tuple、WAL/XL/PL、package、WIT、网络协议和配置文件的版本字段； 构建规范性文件，参考RFC格式和写法，英语放到 `doc/en/contract/`、汉语放到 `doc/cn/contract/`，命名为 `[xxx]_[版本号].md`，在代码的相关位置引用对应的 contract 描述。
- [x] 为每种格式建立版本策略、兼容矩阵、升级规则和废弃周期。
  - 已实现：`mudu/src/compat/mod.rs` 中的 `CompatibilityMatrix`、`FormatKind`、`VersionRange`。
  - 当前注册：page header v1、log frame v1、protocol frame v1、MPK manifest v1（预留）、server config v1（预留）。
- [x] 将已发布版本生成的数据库、日志和 `.mpk` 保存为只读 golden fixtures。
  - 已生成：`testing/fixtures/golden/v1/` 下的 `page_header_v1.bin`、`log_frame_v1.bin`、`protocol_frame_v1.bin`。
  - `.mpk`  golden fixture 待 MPK manifest loader 实现后补充。
- [x] 增加“旧版本写入、新版本读取”的自动化测试。
  - 已实现：`testing/tests/compat_golden.rs` 中的 `golden_v1_roundtrips`。
- [ ] 增加新客户端/旧服务端、旧客户端/新服务端的协议协商测试。
  - 待协议版本协商握手（HandshakeRequest/HandshakeResponse）与多版本服务端实现后补充。
- [x] 增加截断日志、部分写入、校验错误、未知版本和缺失字段测试。
  - 已实现：bad magic、unsupported version、truncated header/payload 的单元与集成测试。
- [ ] 每次格式变更必须附带迁移程序、回滚说明和恢复演练。
  - 兼容矩阵 + contract 已就位。
  - 升级/回滚函数与路由管理器设计见 `doc/cn/todo/format_protocol_upgrade_rollback_todo.md`。
- [x] 为不兼容格式返回稳定错误码，并包含格式类型、实际版本和支持范围。
  - 已实现：`CompatError` 映射到 `UnsupportedFormatVersion` / `CorruptedData` / `IncompatibleProtocolVersion`，错误信息包含 `FormatKind`、实际版本和支持范围。
- [x] 在发布流程中阻止未更新兼容矩阵的格式变更。
  - 已实现：`script/ci/check_format_compatibility.sh` + `.github/workflows/compatibility.yaml`。

## 验收标准

- [x] CI 至少覆盖当前版本和前一个发布版本的读写兼容性。
  - 当前版本 v1 的 golden fixture 测试已加入 CI；前一发布版本 fixture 待发布历史化后补充。
- [x] 所有持久化格式均有明确版本和测试 fixture。
  - page header、log frame、protocol frame 已有；MPK manifest 待实现。
- [x] 升级失败不会静默修改或破坏原数据。
  - 不兼容版本现在返回结构化错误并拒绝解码，不会继续解析。
