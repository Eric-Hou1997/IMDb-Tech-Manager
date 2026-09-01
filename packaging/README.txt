IMDb Tech Manager macOS v4.0.1（Apple Silicon）

正式发布时仅提供 ZIP。ZIP 内仅包含 Apple Silicon arm64 的 IMDb Tech Manager.app。
正式发布文件名为 `ITM-v4.0.1-MacOS-AArch64-APP.zip`；应用包名称保持为 `IMDb Tech Manager.app`，以支持自动替换。

运行要求：macOS 12.0 或更高版本，Apple Silicon。

安装：解压 ZIP 后，将 IMDb Tech Manager.app 拖入“应用程序”文件夹。请勿从 ZIP 内直接运行。

当前构建脚本仅执行本地临时签名，不执行 Apple 公证。不得把源码验证或临时签名描述为已完成正式分发验收；正式发布前必须单独复核 Gatekeeper、签名和公证状态。

此版本修正“关于”区域的正式发布日期，并在每次打开设置页面时自动检查 GitHub 正式更新；检查结果直接显示在“软件更新”区域。
