IMDb Tech Manager macOS v4.1.0（Apple Silicon）

正式发布时仅提供 ZIP。ZIP 内仅包含 Apple Silicon arm64 的 IMDb Tech Manager.app。
正式发布文件名为 `ITM-v4.1.0-MacOS-AArch64-APP.zip`；应用包名称保持为 `IMDb Tech Manager.app`，以支持自动替换。

运行要求：macOS 12.0 或更高版本，Apple Silicon。

安装：解压 ZIP 后，将 IMDb Tech Manager.app 拖入“应用程序”文件夹。请勿从 ZIP 内直接运行。

当前构建脚本仅执行本地临时签名，不执行 Apple 公证。不得把源码验证或临时签名描述为已完成正式分发验收；正式发布前必须单独复核 Gatekeeper、签名和公证状态。

简体中文、繁體中文和 English (United States) 内置于主程序。法语、俄语、日语、西班牙语和泰语以本次 GitHub Release 的独立 ZIP 语言包提供，可在设置中下载；每个包由 v4.1.0 目录绑定并校验，不需要单独更新。升级不会转换或改写已有 NFO、缓存、日志、Ownership、默认系统提示词或用户自定义提示词。

--- English ---

IMDb Tech Manager macOS v4.1.0 (Apple Silicon)

The official release is distributed as a ZIP containing only the Apple Silicon arm64 `IMDb Tech Manager.app`.
The official filename is `ITM-v4.1.0-MacOS-AArch64-APP.zip`. The app-bundle name remains `IMDb Tech Manager.app` so that automatic replacement can work reliably.

Requirements: macOS 12.0 or later on Apple Silicon.

Installation: Extract the ZIP, then drag `IMDb Tech Manager.app` into Applications. Do not run it directly from inside the ZIP.

The current build script performs local ad-hoc signing only and does not notarize the app with Apple. Source validation or ad-hoc signing must not be described as completed distribution acceptance; Gatekeeper behavior, signing, and notarization require separate review before an official release.

Simplified Chinese, Traditional Chinese, and English (United States) are built into the app. French, Russian, Japanese, Spanish, and Thai are separate ZIP language packs attached to this GitHub Release and downloaded from Settings. Each pack is bound to and verified by the v4.1.0 catalog, with no separate update action. Upgrading does not convert or rewrite existing NFO files, caches, logs, Ownership, the default system prompt, or custom prompts.
