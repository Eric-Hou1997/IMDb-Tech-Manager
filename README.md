# IMDb-Tech-Manager v4.0.0

IMDb 技术规格生产、NFO 检查与编辑、标签管理、AI/规则生成和批量任务中心。

当前源码平台：

- `macos/`：Apple Silicon macOS 应用源码。
- `packaging/`：当前版本的应用元数据与发布说明源文件。
- `tools/`：源码验证与发布构建脚本。

本仓库按产品而不是按操作系统划分。未来若增加 Windows 版本，仍应是同一个 IMDb-Tech-Manager 产品，并保留技术规格、NFO、标签和 ownership 的产品职责；它不会与独立的 Tech Card Manager 合并。

## 开发边界

- Technical Specs 是事实层，Tags 是派生层。
- 自动任务只能写入 `<technicalspecs>` 及其明确拥有的 Generated Tech Tags。
- Manual、TMM、普通用户标签和其他应用拥有的标签必须保留。
- 发布前必须运行 `tools/test-source.sh`，并在真实 macOS 环境复验应用生命周期与系统权限行为。
- 发布产物只能是包含 `.app` 的 ZIP；源码验证通过不等于已完成正式发布验收。

本仓库从 v4.0.0 起独立维护，默认开发主线为 `main`。
