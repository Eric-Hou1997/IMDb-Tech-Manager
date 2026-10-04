# IMDb Tech Manager 重写执行记录

- **当前规划入口：[v5 换栈与多平台迁移重新规划](16-replan-2026-09-13.md)**。2026-10-03 正在迁回 ITM 原主窗口、Inspector、预检与审核流程；源码检查和原生启动不代表完整操作验收；TCM 开发与发布阶段已收尾，暂缓的真实运行验收保持未通过。按 ITM v4.1.0 源码补接 Vue/Tauri，复用已有 Rust。当前进展直接更新 [原版纠偏台账](15-baseline-only-correction.md) 和 `features.json`。ITM 仍不提交、打包、发布、标签、GitHub 同步或使用子代理。
- [历史交接：ITM / TCM 原版界面迁移与剩余工作（2026-09-12）](11-handoff-2026-09-12.md)。其中已被后续用户指令替代的执行授权不再适用。

- [交付范围与重写规则](00-scope.md)
- [功能台账](01-features.md) / [机器台账](features.json)
- [基线验证方法](02-baseline.md)
- [平台验收与阻塞](03-platforms.md)
- [可重复执行入口](../../rewrite/README.md)

完成状态必须以执行证据为准；不能把入口覆盖检查或工程骨架当作功能迁移完成。

- [前四步状态与证据](04-status.md)
- [前置调查和阻塞](05-findings.md)

- [原始数据与迁移契约](06-data-contracts.md)
- [历史验证包的 IPC/退出证据（不代表当前原版界面验收）](evidence/native-smoke.json)

- [第 5–8 步实现、验证和剩余工作](07-implementation.md)

- [维护者模拟 NFO 库：101 份样本验收](08-sample-library.md)

- [云端验收与本地交付进度](09-cloud-validation.md)

- [历史第 9–12 步清单及证据](10-completion-checklist.md)：保留查证，不作为当前执行顺序或授权；冲突要求以当前规划及最新用户指令为准。
