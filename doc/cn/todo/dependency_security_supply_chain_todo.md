# 依赖安全与软件供应链 TODO

## 目标

建立安全公告、第三方依赖和发布制品的持续治理机制。

## TODO

- [ ] 为 `deny.toml` 中每个 advisory ignore 项补充 owner、跟踪 Issue、影响分析和到期日期。
- [ ] 到期豁免必须阻塞 CI，续期需要重新评估风险。
- [ ] 建立 Wasmtime、libsql、TLS 和数据库驱动等关键依赖的升级计划。
- [ ] 固定 GitHub Actions 到 commit SHA，并定期自动更新。
- [ ] 审计 Git 依赖来源，固定 revision 并记录替换 crates.io 版本的原因。
- [ ] 发布时生成 CycloneDX 或 SPDX SBOM。
- [ ] 为发布归档、校验文件和 SBOM 增加签名与 provenance。
- [ ] 增加 `SECURITY.md`，说明支持版本、漏洞报告渠道和响应流程。
- [ ] 定期执行最小依赖与 feature 审查，移除无用或过宽 feature。

## 验收标准

- [ ] 不存在无负责人、无期限的安全公告豁免。
- [ ] 发布制品可验证来源、完整性和依赖清单。
- [ ] 高风险依赖升级有固定周期和回归测试。

