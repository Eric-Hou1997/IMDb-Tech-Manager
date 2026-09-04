IMDb Tech Manager macOS v4.0.4（Apple Silicon）

正式发布时仅提供 ZIP。ZIP 内仅包含 Apple Silicon arm64 的 IMDb Tech Manager.app。
正式发布文件名为 `ITM-v4.0.4-MacOS-AArch64-APP.zip`；应用包名称保持为 `IMDb Tech Manager.app`，以支持自动替换。

运行要求：macOS 12.0 或更高版本，Apple Silicon。

安装：解压 ZIP 后，将 IMDb Tech Manager.app 拖入“应用程序”文件夹。请勿从 ZIP 内直接运行。

当前构建脚本仅执行本地临时签名，不执行 Apple 公证。不得把源码验证或临时签名描述为已完成正式分发验收；正式发布前必须单独复核 Gatekeeper、签名和公证状态。

此版本新增 Web、Go Core、Python Engine 与 macOS 原生界面的简体中文 / English (United States) 双语支持，并完成响应式布局、升级兼容和全量回归收口。默认系统提示词与用户自定义提示词保持不变。

--- English ---

IMDb Tech Manager macOS v4.0.4 (Apple Silicon)

The official release is distributed as a ZIP containing only the Apple Silicon arm64 `IMDb Tech Manager.app`.
The official filename is `ITM-v4.0.4-MacOS-AArch64-APP.zip`. The app-bundle name remains `IMDb Tech Manager.app` so that automatic replacement can work reliably.

Requirements: macOS 12.0 or later on Apple Silicon.

Installation: Extract the ZIP, then drag `IMDb Tech Manager.app` into Applications. Do not run it directly from inside the ZIP.

The current build script performs local ad-hoc signing only and does not notarize the app with Apple. Source validation or ad-hoc signing must not be described as completed distribution acceptance; Gatekeeper behavior, signing, and notarization require separate review before an official release.

This version adds Simplified Chinese and English (United States) across the Web UI, Go Core, Python Engine, and native macOS interface, with responsive-layout, upgrade-compatibility, and full-regression closeout. The default system prompt and every custom prompt remain unchanged.
