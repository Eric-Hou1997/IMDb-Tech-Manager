IMDb Tech Manager v5.0.0

本版仅将 v4.1.0 迁移至 Rust + Tauri 2 + TypeScript + Vue 3/Vite，并适配约定平台。原源码与有效测试保留；不新增产品功能。

五目标、九种主安装包：
- macOS ARM64：ITM-v5.0.0-MacOS-AArch64.dmg
- Windows x64：ITM-v5.0.0-Windows-x64-Setup.exe
- Windows ARM64：ITM-v5.0.0-Windows-AArch64-Setup.exe
- Linux x64：ITM-v5.0.0-Linux-x64.AppImage / .deb / .rpm
- Linux ARM64：ITM-v5.0.0-Linux-AArch64.AppImage / .deb / .rpm
没有 macOS Intel 包。包名中的架构是应用架构。

安装：
macOS：打开 DMG，将 IMDb Tech Manager.app 拖入应用程序目录；从安装后的目录运行。
Windows：运行对应架构的 Setup.exe，按 NSIS 安装流程完成安装；系统需要 WebView2 运行时。
Linux：AppImage 赋予可执行权限后运行；DEB/RPM 使用系统包管理器安装，解决所需运行依赖。不要同时安装多个渠道后混用更新文件。

升级前请正常退出旧版，备份配置、缓存、NFO、归属镜像及撤销资料。v5 保留旧双目录数据、提示词、凭据引用、历史和语言包的恢复链；原目录和旧日志不删除或改写。旧后台仍运行、源变化、归属不明或恢复失败时，启动失败关闭并要求修正/重试。真实旧安装/登录项接管操作尚未验收。

简体中文、繁體中文与 English (United States) 内置。法语、俄语、日语、西班牙语及泰语是同一 Release 的五个 r2 ZIP，在原语言选择器下载并按精确目录摘要验证；已有 r1 可在离线恢复时复用，旧日志不重写。语言包不能改变 NFO、Technical Specs、Tag、Ownership 或提示词。

更新：v4.1.0 使用 ITM-v5.0.0-MacOS-AArch64-APP.zip 与其原格式 .zip.sig；应用包名称和 Contents/MacOS/IMDbTechManagerLauncher 保持不变，主程序已经是 Rust。v5 macOS 更新使用同基名 .tar.gz 与 Minisign 签名；Windows NSIS、Linux AppImage 使用匹配架构的文件和签名。DEB/RPM 从同一 Release 下载并使用系统包管理器升级；本版没有新增软件源。ITM-update.json 只绑定当前确切资产，SHA-256 与现有 ITM Ed25519 信任根共同校验。

校验：下载 ITM-v5.0.0-SHA256SUMS.txt，比较文件摘要，例如 macOS 使用 shasum -a 256，Windows 使用 Get-FileHash -Algorithm SHA256，Linux 使用 sha256sum。LICENSE 与 NOTICE 随每个包分发；许可证为 Apache License 2.0，作者为侯雁泽。

验收与签名边界：macOS 为临时签名，未做 Apple 公证；Windows 没有发行证书签名。Ed25519 OTA 签名与 Apple/Windows 分发证书不是同一机制。macOS 12+、Windows 11 24H2 与 Linux 发行版矩阵是验收目标，不能由编译结果推定支持。维护者本次暂缓完整原生操作、像素与真实 AI/IMDb 流程；Windows/Linux 实机及最低 macOS 运行未认证。源码回归、架构/合法文件包内容核对不代替这些项目。

--- English ---

IMDb Tech Manager v5.0.0

This release migrates v4.1.0 to Rust + Tauri 2 + TypeScript + Vue 3/Vite and adapts the agreed platforms. Original source and valid tests remain; no product features are added.

Five targets and nine primary packages:
- macOS ARM64: ITM-v5.0.0-MacOS-AArch64.dmg
- Windows x64: ITM-v5.0.0-Windows-x64-Setup.exe
- Windows ARM64: ITM-v5.0.0-Windows-AArch64-Setup.exe
- Linux x64: ITM-v5.0.0-Linux-x64.AppImage / .deb / .rpm
- Linux ARM64: ITM-v5.0.0-Linux-AArch64.AppImage / .deb / .rpm
There is no macOS Intel package. Filename architecture identifies the application architecture.

Installation:
macOS: Open the DMG, drag IMDb Tech Manager.app into Applications, and run the installed copy.
Windows: Run the matching NSIS Setup.exe; the system requires the WebView2 runtime.
Linux: Make the AppImage executable before running it, or install DEB/RPM with the system package manager and satisfy runtime dependencies. Avoid mixing update files from different installation channels.

Before upgrading, exit the old application normally and back up settings, caches, NFO files, ownership mirrors and undo records. v5 retains recovery for both old data directories, prompts, credential references, history and language packs; it does not remove the originals or rewrite old logs. Running old writers, changed sources, uncertain ownership and recovery failures block startup until corrected/retried. Real old-installation and login-item takeover operations remain unverified.

Simplified Chinese, Traditional Chinese and English (United States) are built in. French, Russian, Japanese, Spanish and Thai are five r2 ZIP assets on the same Release, downloaded through the original language picker and checked against exact catalog digests. Installed r1 packs can be reused during offline restoration; old logs remain unchanged. Packs cannot alter NFO, Technical Specs, Tags, Ownership or prompts.

Updates: v4.1.0 uses ITM-v5.0.0-MacOS-AArch64-APP.zip and its original-format .zip.sig. The app name and Contents/MacOS/IMDbTechManagerLauncher remain stable; that launcher is now the Rust application. v5 macOS uses the same base with .tar.gz and a Minisign signature. Windows NSIS and Linux AppImage use exact matching architectures and signatures. Download DEB/RPM from this Release and upgrade with the system package manager; no software repository is added. ITM-update.json binds exact assets, checked with SHA-256 and the existing ITM Ed25519 trust key.

Integrity: Download ITM-v5.0.0-SHA256SUMS.txt and compare digests using shasum -a 256 on macOS, Get-FileHash -Algorithm SHA256 on Windows, or sha256sum on Linux. Every package includes LICENSE and NOTICE. The license is Apache License 2.0; the author is 侯雁泽.

Acceptance and signing: macOS is ad hoc signed and not Apple-notarized; Windows has no distribution-certificate signature. Ed25519 OTA signing is separate from Apple/Windows distribution signing. macOS 12+, Windows 11 24H2 and the Linux distribution matrix are acceptance targets, not support inferred from compilation. The maintainer deferred full native operation, pixel and real AI/IMDb workflow acceptance. Windows/Linux hardware and minimum-macOS operation are uncertified. Source regressions, architecture and packaged legal-file checks do not replace those items.
