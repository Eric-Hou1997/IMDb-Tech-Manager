<div align="center">

# IMDb Tech Manager

**简体中文** | [English](./README.en.md)

<p align="center">

[![Release](https://img.shields.io/github/v/release/Eric-Hou1997/IMDb-Tech-Manager?label=release)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Downloads](https://img.shields.io/github/downloads/Eric-Hou1997/IMDb-Tech-Manager/total?label=downloads)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Stars](https://img.shields.io/github/stars/Eric-Hou1997/IMDb-Tech-Manager?style=flat&logo=github)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/stargazers)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/pulls)

</p>

影视技术规格与媒体库元数据管理工具

---

## 🎬 项目简介

**IMDb Tech Manager** 是一个面向影视作品技术信息管理的项目。

</p>

<img src="./docs/images/poster.jpg" alt="IMDb Tech Manager Poster" width="700">

</div>

</p>

项目主要关注 **IMDb Technical Specifications（IMDb 技术规格）** 的获取、结构化、标准化与应用，将原本比较分散的影视制作技术资料转化为可以在个人媒体库中管理、检索和展示的元数据。

目前主要关注：

* IMDb Technical Specifications
* NFO 元数据管理
* 技术标签生成
* 摄影机与镜头信息
* 胶片与数字采集格式
* 制作与放映规格
* 技术元数据标准化
* 媒体库技术信息展示
* AI 辅助语义处理
* Coding Agent 辅助开发

现在的大多数媒体库已经可以很好地展示片名、演员、年份、分辨率、编码格式和音频格式。
但一部影视作品**使用了什么摄影机、什么镜头、采用什么胶片或数字采集格式、经过怎样的制作流程，以及最终以什么规格完成和放映**，通常并没有得到完整、结构化的保存和展示。

IMDb Tech Manager 希望把这些信息真正带进个人影视媒体库的工作流中。

---
## 🖼️ 界面预览

### 数据管理

<div align="center">

<img src="./docs/images/data-management.png" alt="IMDb Tech Manager Data Management" width="700">

</div>

</p>

nfo数据管理端用于 IMDb Technical Specifications 获取、技术规格整理、标签生成以及批量任务管理。

### NFO 管理

<div align="center">

<img src="./docs/images/tag-management.png" alt="IMDb Tech Manager Tag Management" width="700">

</div>

</p>

Tag 管理功能用于检查、预演和修改媒体库中的技术元数据，并允许用户对自动生成的内容进行手动修正。

### AI Runtime 管理

<div align="center">

<img src="./docs/images/ai-runtime-management.png" alt="IMDb Tech Manager AI Runtime Management" width="700">

</div>

</p>

AI Runtime 管理界面用于配置模型接口、API Base URL、Prompt Cache、推理参数、系统提示词以及 AI 标签生成相关行为。

### 媒体库技术规格展示

<div align="center">

<img src="./docs/images/media-library-card.png" alt="IMDb Tech Manager Media Library Technical Specifications" width="900">

</div>

</p>

经过处理的技术规格可以进一步用于媒体库中的技术信息展示。

---

## 🔄 核心工作流

```text
IMDb Technical Specifications
        ↓
数据获取与结构化
        ↓
技术规格标准化
        ↓
NFO 元数据管理
        ↓
技术标签生成
        ↓
媒体库中的技术信息展示
```

项目的目标并不是简单保存 IMDb 页面上的原始文字，而是希望把这些技术规格转化为可以：

* 管理
* 标准化
* 手动修正
* 搜索
* 标签化
* 展示
* 被其他工具继续利用

的结构化信息。

---
## 📚 主要技术信息

IMDb Technical Specifications 中包含大量影视制作技术资料，例如：

* 📷 摄影机（Cameras）
* 🔭 摄影镜头（Lenses）
* 🎞️ 胶片采集格式
* 💾 数字采集格式
* 🎥 摄影工艺（Cinematographic Process）
* 🧪 实验室与后期制作流程
* 🖼️ 画幅比例（Aspect Ratio）
* 🔊 声音格式（Sound Mix）
* 📽️ 母版与放映格式
* 🎬 Printed Film Format
* 其他与制作相关的 Technical Specifications

IMDb Tech Manager 将围绕这些数据建立解析、标准化、元数据管理和展示能力。

---

## 🧩 项目架构

IMDb Tech Manager（ITM）与 [Tech Card Manager（TCM）](https://github.com/Eric-Hou1997/Tech-Card-Manager) 围绕同一套 **Technical Specifications** 工作流协同工作。

两者分别负责不同阶段：

* **ITM** 负责技术规格的获取、处理、检查与元数据维护
* **TCM** 负责将已经处理好的技术规格应用到媒体库（如在Emby的界面中生成相应的技术规格卡片），并完成展示与集成

两者是相互配合的独立工具，也可以根据实际需求分别使用和独立发展。

### 📦 IMDb Tech Manager（ITM）

**IMDb Tech Manager（ITM）** 主要负责 Technical Specifications 的数据管理与处理。

主要职责包括：

* IMDb Technical Specifications 数据获取
* Technical Specifications 结构化
* 技术规格标准化
* NFO 文件管理
* Technical Specifications 写入 NFO
* 技术标签生成
* AI 辅助语义处理
* Preview / Dry Run
* 手动修正
* 批量处理
* 元数据维护

ITM 负责将原始技术规格整理成稳定、结构化、可维护的媒体元数据。

这些经过 ITM 处理后的 Technical Specifications，可以进一步交由 **TCM** 使用，在媒体库中完成技术规格卡片生成、展示与同步。

### 🖥️ [Tech Card Manager（TCM）](https://github.com/Eric-Hou1997/Tech-Card-Manager)

**Tech Card Manager（TCM）** 主要负责 Technical Specifications 在媒体库中的展示与集成。

主要职责包括：

* 读取 ITM 或其他兼容数据源提供的 Technical Specifications
* 技术规格卡片生成与维护
* 技术信息展示
* 元数据同步
* Web UI 集成
* 不同媒体类型的兼容处理
* 与媒体库元数据工作流联动

TCM 与 ITM 使用相同的 Technical Specifications 数据体系，但职责有所区分。

ITM 更关注技术信息本身的获取、整理和维护，TCM 则负责将这些已经整理好的数据应用到实际媒体库环境中。

因此，两者可以形成一套完整的工作流：

**IMDb → ITM → NFO / Technical Specifications → TCM → 媒体库展示**

### 🧭 平台关系

ITM 与 TCM 的职责划分**不与某一个操作系统或媒体服务器永久绑定**。

目前实际已经实现或正在重点开发的是：

* **ITM**：当前主要运行于 **macOS** 的 Technical Specifications 数据管理工具
* **TCM**：当前主要围绕 **Emby** 开发的 Technical Specifications 卡片管理与媒体库集成工具

这些只是现阶段的实现形态，并不代表两个项目未来只能运行在这些平台上。

后续可以继续扩展：

* 其他操作系统
* 其他媒体服务器
* 其他部署方式
* 其他客户端
* 其他兼容的 Technical Specifications 数据源

ITM 与 TCM 在架构上保持相对独立，通过标准化的 Technical Specifications 与媒体元数据进行衔接，从而为后续扩展不同平台和媒体库提供空间。

## 🧠 项目设计原则

IMDb Tech Manager 的开发重点之一，是让自动化能力保持**可控、可检查、可回退**。

无论使用本地规则还是 AI，最终目标都是可靠地维护用户的媒体元数据，而不是单纯追求自动化程度。

### 1. 确定性规则优先

对于可以通过明确规则稳定解决的问题，优先使用本地确定性逻辑。

例如：

* 已知格式映射
* 技术规格标准化
* 标签规则
* 数据结构转换
* NFO 读取与写入
* 文件处理
* 数据验证
* 所有权判断

这类任务具有明确输入和明确结果，使用确定性规则通常更容易测试、复现和验证。

不会为了使用 AI，而把能够可靠解决的问题交给模型处理。

### 2. AI 处理模糊语义

AI 主要用于固定规则难以完整覆盖的语义问题，例如：

* 不规则自然语言
* 有歧义的技术规格
* 复杂语义拆分
* 厂商、系列与型号关系判断
* 多种表达方式的归一化
* 需要结合上下文理解的技术信息

AI 是 IMDb Tech Manager 的一个能力模块。

它负责补充规则系统不擅长的部分，而不会取代整个数据处理流程。

### 3. AI Provider 可替换

模型层保持可配置，当前主要通过类似下面的配置接入不同模型服务：

```text
Provider
Base URL
API Key
Model
```

这样可以根据：

* 模型能力
* 调用成本
* 可用性
* API 兼容性
* 部署环境
* 隐私要求

切换不同的模型服务。

项目不会把核心数据工作流永久绑定到某一个 AI Provider。

### 4. 修改前先预演

涉及 NFO 和媒体元数据修改的操作，尽可能先提供可检查的结果，再执行正式写入。

典型流程：

```text
Preview / Dry Run
        ↓
检查修改结果
        ↓
正式执行
        ↓
验证写入结果
```

尤其适用于：

* NFO 修改
* Technical Specifications 写入
* 技术标签生成
* 技术标签更新
* 批量操作
* 元数据迁移

对于批量修改，能够提前看到“将要发生什么”比单纯提高执行速度更重要。

### 5. 尊重用户维护的数据

IMDb Tech Manager 会区分不同来源和所有权的元数据。

自动化流程只能修改自己能够明确确认所有权的数据，不应该因为文本相似，就推断某个标签属于 IMDb Tech Manager。

特别是：

* 用户手动维护的标签
* 外部工具生成的标签
* TMM 等其他应用维护的数据
* 无法确认来源的数据

都不应该被后台任务擅自删除、接管或覆盖。

用户手动修改过的 Generated Tech Tag 会被视为需要保护的人工数据。

### 6. Technical Specifications 与 Tags 分层

项目将 **Technical Specifications** 视为事实数据层，将 **Tags** 视为基于事实数据进一步生成的派生信息。

```text
Technical Specifications
        ↓
标准化 / 语义处理
        ↓
Technical Tags
```

因此：

* Spec Agent 只负责 Technical Specifications
* 标签生成器负责 Technical Tags
* 修改 Technical Specifications 不应该隐式触发 IMDb 数据刷新
* 标签重建应该是独立、可观察的操作

这样可以避免不同处理阶段互相污染，也方便后续重新生成标签或接入其他工具。

### 7. 写入操作必须安全

NFO 是媒体库的重要长期数据，因此修改流程需要尽量避免“写了一半”“误删标签”或“文件损坏”这类问题。

项目会持续强化：

* XML 有效性检查
* 修改前备份
* 原文件状态检查
* 原子写入
* 路径安全检查
* 标签所有权验证
* 冲突检测
* 失败时保持原文件不变
* 可恢复与可撤销能力

当系统无法安全判断是否应该修改某项数据时，默认选择跳过，而不是冒险写入。

### 8. 后台行为可观察、可验证

重要后台操作应该尽可能向用户提供明确状态。

例如：

* 正在处理什么
* 修改了什么
* 哪些结果来自本地规则
* 哪些结果来自 AI
* API 调用次数
* Token 使用情况
* Cache 命中情况
* 估算费用
* 当前任务阶段
* 成功 / 失败 / 跳过状态
* 错误原因
* 受影响的影片或 NFO

项目尽量避免只显示一个简单的“成功”，而忽略中间实际发生了什么。

---

## 🤖 面向 Coding Agent 的开发

IMDb Tech Manager 的公开 Repository 已经提供：

[**`AGENTS.md` →**](./AGENTS.md)

它是 Codex 等 Coding Agent 理解和修改项目的重要上下文入口。

`AGENTS.md` 目前已经包含：

* Repository 身份与源码边界
* 当前产品职责
* 软件架构约束
* NFO 安全规则
* 标签所有权规则
* AI 调用与 Token 统计规则
* 生命周期管理要求
* 测试与验证要求
* Release 边界
* OTA 签名要求
* 平台支持边界
* 不应该进行的修改
* 修改完成前需要执行的检查

开发者 Fork 或 Clone 项目后，可以让支持 Repository Context 的 Coding Agent 先读取 `AGENTS.md`，再开始分析和修改代码。

推荐工作流：

```text
Fork / Clone
        ↓
Coding Agent 读取 AGENTS.md
        ↓
读取相关源码与测试
        ↓
理解模块职责与修改边界
        ↓
分析受到影响的功能链路
        ↓
制定修改方案
        ↓
修改代码
        ↓
运行相关测试
        ↓
验证真实行为
        ↓
提交 Pull Request
```

IMDb Tech Manager 希望 Repository 提供的不只是源码本身：

```text
源代码
  +
架构知识
  +
设计约束
  +
开发规则
  +
测试方法
  +
Agent Context
```

这样无论是开发者直接阅读代码，还是使用 Codex 等 Coding Agent，都可以更快理解项目，同时减少“代码能运行，但破坏了既有设计”的修改。

---

## 🚧 当前状态

IMDb Tech Manager 目前处于 **公开源码、持续开发阶段**。

公开源码从 **v4.0.0** 开始维护。

当前 Repository 已经包含：

* 完整项目源码
* macOS 当前实现
* `AGENTS.md`
* 测试代码
* Release 构建工具
* Packaging 配置
* 项目文档
* `LICENSE`
* `NOTICE`
* `SECURITY.md`
* `PRIVACY.md`
* `TERMS.md`

### 当前支持平台

目前正式维护的实现为：

**macOS · Apple Silicon（arm64）**

当前桌面端主要由以下部分组成：

```text
Native Launcher
      +
Go Core
      +
Local Web UI
      +
Python Engine
```

Repository 按产品组织，而不是永久按照操作系统划分。

未来如果扩展 Windows 或其他平台，仍然属于 IMDb Tech Manager，只要保持相同的产品职责和数据工作流。

### 当前正式版本

当前公开版本：

**IMDb Tech Manager v4.0.0**

Release 已提供 Apple Silicon `.app` 的 ZIP 发布包，并同时提供：

* Release Changelog
* SHA-256 校验信息
* OTA Ed25519 签名
* 安装说明

[**查看 Releases →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)

项目仍然处于持续开发阶段，功能、架构、测试覆盖和平台支持都会继续迭代。

---

## 🗺️ Roadmap

### 已完成

* [x] 建立公开 Repository
* [x] 开放核心源码
* [x] 建立项目基础目录结构
* [x] 发布 `AGENTS.md`
* [x] 确定开源许可证
* [x] 建立基础测试体系
* [x] 建立 Release 构建流程
* [x] 发布首个公开源码版本 `v4.0.0`
* [x] 建立 macOS Apple Silicon 发布流程
* [x] 加入 Release 完整性校验与 OTA 签名

### 持续推进

* [ ] 完善 Technical Specifications 标准化规则
* [ ] 扩展更多 Technical Specifications 数据处理能力
* [ ] 完善 Local / AI 技术标签生成
* [ ] 强化 NFO 数据所有权与安全机制
* [ ] 扩展自动化测试与真实场景回归测试
* [ ] 改进任务状态、错误定位与恢复能力
* [ ] 完善 AI Provider 与模型配置能力
* [ ] 优化 Token、Cache 与调用成本统计
* [ ] 完善应用更新与 Release 工作流
* [ ] 完善 Developer ID 签名与 macOS 分发体验
* [ ] 持续完善 `AGENTS.md` 与 Coding Agent Context
* [ ] 完善贡献与 Pull Request 工作流
* [ ] 探索其他操作系统支持
* [ ] 与 [Tech Card Manager（TCM）](https://github.com/Eric-Hou1997/Tech-Card-Manager) 完善 Technical Specifications 工作流衔接
* [ ] 探索更多媒体服务器与客户端的集成方式

Roadmap 会随着项目开发和实际使用反馈持续调整。

---

## 💬 Discussions

功能想法、技术方案、UI 设计、Technical Specifications 标准化规则以及开发工作流，都欢迎在 Discussions 中交流：

[**进入 GitHub Discussions →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/discussions)

适合 Discussions 的内容包括：

* 新功能想法
* 技术方案讨论
* Technical Specifications 数据规则
* 摄影机 / 镜头 / 胶片 / 制作格式资料
* UI / UX 建议
* AI 标签生成策略
* ITM 与 TCM 的工作流
* Coding Agent 开发方式
* 尚未完全确定的问题

如果一个想法还需要进一步讨论和验证，可以先放到 Discussions，而不必立即整理成 Issue。

---

## 🐛 Issues

对于已经能够明确描述的问题，可以直接提交 Issue：

[**GitHub Issues →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/issues)

例如：

* 可以稳定复现的 Bug
* 明确的功能缺失
* 数据解析错误
* Technical Specifications 标准化错误
* NFO 修改异常
* UI 行为异常
* Release / 安装问题
* 已经比较清晰的功能需求

提交问题时，如果条件允许，建议同时提供相关版本、媒体类型、操作步骤和错误信息，以便更快定位问题。

---

## 🤝 Contributing

IMDb Tech Manager 已经开放源码，欢迎 Fork、研究、修改并提交 Pull Request。

开始修改代码之前，建议先阅读：

[**`AGENTS.md` →**](./AGENTS.md)

它记录了项目当前的重要架构规则、数据安全约束、测试要求和 Release 边界。

尤其是在修改以下模块时，请先了解现有设计：

* NFO 读写
* Technical Specifications
* Technical Tags
* 标签所有权
* AI 调用
* Token / Cache 统计
* 批量任务
* 应用生命周期
* 平台相关代码
* 更新与 Release

项目欢迎包括但不限于以下类型的贡献：

* Bug 修复
* 功能改进
* Technical Specifications 解析规则
* 技术规格标准化
* 摄影机 / 镜头 / 制作格式资料
* 测试用例
* UI / UX 改进
* 性能与稳定性改进
* 文档完善
* Coding Agent Context 改进

在修改已有行为时，请尽量补充对应的测试或回归验证，避免修复一个问题的同时破坏已有媒体数据工作流。

---

## 📄 License

IMDb Tech Manager 采用 **Apache License 2.0** 开源。

完整许可证请查看：

[**LICENSE →**](./LICENSE)

Repository 同时提供：

[**NOTICE →**](./NOTICE)

使用、修改和分发源码时，请遵守 Apache License 2.0 以及 Repository 中相关说明。

---

## ⚠️ 免责声明

IMDb Tech Manager 是一个独立开发的开源项目。

本项目**与 IMDb、Emby 以及其他第三方平台不存在官方隶属、授权或背书关系**。

相关第三方名称、商标、数据和服务归各自权利方所有。

IMDb Tech Manager 提供的是技术规格获取、处理和媒体元数据管理工具。

用户在使用第三方数据、API、网站或服务时，应自行确认相关使用方式符合适用的服务条款、授权条件和法律要求。

---

## 💡 反馈与建议

IMDb Tech Manager 仍在持续开发。

如果你对以下方向有想法：

* IMDb Technical Specifications
* 技术规格标准化
* 摄影机与镜头资料
* 胶片与数字采集格式
* NFO 元数据管理
* 技术标签规则
* AI 辅助语义处理
* UI / UX
* ITM 与 TCM 的协作方式
* 其他操作系统支持
* Coding Agent 开发工作流

欢迎通过 Discussions 或 Issues 参与项目。

项目会根据实际使用反馈继续调整功能、数据规则和开发方向。
