# IMDb Tech Manager

影视技术规格与媒体库 NFO 管理工具。IMDb Tech Manager 将摄影机、镜头、胶片与数字采集格式、画幅、声音格式等技术资料整理为可检查、可维护的 NFO 数据与标签。

源码自 v4.0.0 起在 `main` 独立维护，采用 [Apache License 2.0](LICENSE)。作者：侯雁泽。

## 界面预览

<p align="center">
  <img src="./docs/images/data-management.png" alt="IMDb Tech Manager 数据管理" width="700">
</p>

<p align="center">
  <img src="./docs/images/tag-management.png" alt="IMDb Tech Manager NFO 管理" width="700">
</p>

<p align="center">
  <img src="./docs/images/ai-runtime-management.png" alt="IMDb Tech Manager AI Runtime 管理" width="700">
</p>

## 核心工作流

```text
IMDb Technical Specifications
        ↓
结构化与标准化
        ↓
NFO Technical Specs
        ↓
AI / 规则预演
        ↓
经确认后写入派生技术标签
```

主要能力：

- 管理电影与电视剧 NFO，并保持两者的搜索、筛选、选择和任务记录彼此独立。
- 将 Technical Specs 作为事实层；标签是可重新生成的派生层。
- 通过 AI 或确定性规则生成标签，并在写入前提供预演与确认。
- 用嵌入式 ownership 元数据区分 Generated、Manual 与 External 标签；不根据文字猜测归属。
- 在批量任务中提供真实进度、暂停、恢复、失败重试和可追溯错误信息。

## 数据安全原则

- 自动任务只写入 `<technicalspecs>` 及其明确拥有的 Generated Tech Tags。
- Manual、tinyMediaManager、普通用户及其他应用拥有的标签必须保留。
- 归属不明时安全跳过，绝不推断删除。
- 写入使用备份、完整候选校验、原子替换和来源哈希校验。

## 源码结构

- `macos/`：Apple Silicon macOS 应用、Go Core、本地 Web UI 与 Python 引擎。
- `packaging/`：应用元数据、发布说明与签名辅助脚本。
- `tools/`：源码验证和发布构建脚本。
- `docs/images/`：GitHub 页面使用的图片素材。

本仓库按产品而非操作系统划分。将来新增 Windows 版本时，仍属于 IMDb Tech Manager；它不会与独立的 Tech Card Manager 合并。

## 开发与验证

```bash
tools/test-source.sh
```

该命令覆盖 Go、Python、Web UI 与源码契约。正式发布前还必须在真实 macOS 环境验证应用生命周期、权限与更新流程；源码测试通过不等于完成发布验收。

发布产物仅可为包含 `.app` 的 ZIP。设置页可检查 GitHub 正式发布；更新包必须通过 Ed25519 签名验证后才能自动替换当前应用。未使用 Apple Developer ID 签名时，macOS 仍可能要求用户在“隐私与安全性”中手动选择“仍要打开”。

## 贡献

欢迎提交 Issue、改进建议和 Pull Request。仓库协作与产品安全边界见 [AGENTS.md](AGENTS.md)。

## 文档与声明

- [隐私政策](PRIVACY.md)
- [使用条款](TERMS.md)
- [安全与更新说明](SECURITY.md)
- [开源通知](NOTICE)

## 免责声明

本软件为独立开发工具，与 IMDb.com, Inc. 或 tinyMediaManager 无隶属、授权或背书关系。相关商标归各自权利人所有。
