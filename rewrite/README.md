# ITM v5.0.0 功能与界面迁移工程

目标是 Rust + Tauri 2 + TypeScript + Vue 3/Vite 运行原产品；以 `macos/web/index.html` 为界面和行为基线，原文件位于仓库根目录下。保留 IMDb 规格、NFO Inspector/编辑、归属、规则/AI、任务、缓存、备份撤销及原更新能力。 复用已有实现，不重新搭建验证工作台。完整 UI、业务与平台验收仍未完成。

独立应用 ID 和数据目录继续用于开发隔离；v5.0.0 是当前换栈版本，v4.1.0 是功能、界面和操作基线。当前执行约束和状态见 [重写规划入口](../docs/rewrite/README.md)，旧 12 步与旧验收记录只作查证。

## 开发与验证

使用仓库锁定的工具链和依赖。依赖已安装时直接复用；需要恢复 Node 依赖时运行 `npm ci`。原生开发入口：

```sh
npm run tauri dev -- --no-watch
```

仅 `npm run dev` 的浏览器页面没有 Tauri IPC，只可检查布局。原生开发必须验证窗口可操作、实际 IPC、对应原用户流程及退出清理；进程启动或探针成功不等于界面已显示。使用隔离数据，避免触碰生产媒体和正式应用。

按修改范围选择检查。例如移除废弃 IPC 包装层时，可以检查桌面命令编译和保留的数据兼容测试：

```sh
cd src-tauri
cargo check --locked -p itm-validation --bin itm-validation
cargo test --locked -p itm-core --features write-prototype --test migration --test history
```

前端修改运行相关前端回归与类型检查；核心修改运行对应行为和安全回归。不要把每个小改动自动升级为无关全量构建。受影响的平台和 ARM64 检查随开发推进，缺少实际环境时明确记录，不用交叉编译代替实机。

`REWRITE_PROBE_REPORT` / `REWRITE_PROBE_AUTOCLOSE` 是现有隔离验收工具；保留必要验证用途，不进入正常产品 UI，也不据此声称像素或业务通过。探针输出不得覆盖有价值的文件。

## 交付边界

每产品 5 目标、9 包：macOS ARM64 DMG，Windows x64/ARM64 NSIS，Linux x64/ARM64 AppImage/DEB/RPM；无 macOS Intel。维护者现已授权 ITM 五目标 GitHub 构建、制包、保护性清理、提交/同步和 v5.0.0 发布；完整原生操作/像素与真实 AI/IMDb 流程暂缓，不作为通过。不使用子代理。正式构建设置 `ITM_RELEASE_BUILD=1`，合并 `src-tauri/tauri.release.conf.json`；macOS 另合并 `src-tauri/tauri.release.macos.conf.json`，保留旧 OTA 的启动器文件名。开发配置继续隔离。已有配方、有效测试、旧源码和用户数据保持；仅清理确认可再生成且无进程占用的文件。

数据兼容后端按原版正常入口接线；不要恢复已移除的通用数据导入、独立历史浏览面板或其 Tauri 命令。底层迁移/历史读取和有效回归继续保留；它们的存在不等于完整旧数据迁移已通过。
