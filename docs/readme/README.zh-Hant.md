<div align="center">

# IMDb Tech Manager

[簡體中文](../../README.md) | **繁體中文** | [English](./README.en.md) | [Français](./README.fr.md) | [Русский](./README.ru.md) | [日本語](./README.ja.md) | [Español](./README.es.md) | [ไทย](./README.th.md)

<p align="center">

[![Release](https://img.shields.io/github/v/release/Eric-Hou1997/IMDb-Tech-Manager?label=release)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Downloads](https://img.shields.io/github/downloads/Eric-Hou1997/IMDb-Tech-Manager/total?label=downloads)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Stars](https://img.shields.io/github/stars/Eric-Hou1997/IMDb-Tech-Manager?style=flat&logo=github)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/stargazers)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/pulls)

</p>

<img src="../../macos/assets/ITM_logo.png" alt="IMDb Tech Manager" width="220">

影視技術規格與媒體庫元資料管理工具

---

## 🎬 項目簡介

**IMDb Tech Manager** 是一個面向影視作品技術資訊管理的項目。

</p>

<img src="../images/poster.jpg" alt="IMDb Tech Manager Poster" width="700">

</div>

</p>

項目主要關注 **IMDb Technical Specifications（IMDb 技術規格）** 的獲取、結構化、標準化與應用，將原本比較分散的影視製作技術資料轉化為可以在個人媒體庫中管理、檢索和展示的元資料。

目前主要關注：

* IMDb Technical Specifications
* NFO 元資料管理
* 技術標籤生成
* 攝影機與鏡頭資訊
* 膠片與數字採集格式
* 製作與放映規格
* 技術元資料標準化
* 媒體庫技術資訊展示
* AI 輔助語義處理
* Coding Agent 輔助開發

現在的大多數媒體庫已經可以很好地展示片名、演員、年份、分辨率、編碼格式和音頻格式。
但一部影視作品**使用了什麼攝影機、什麼鏡頭、採用什麼膠片或數字採集格式、經過怎樣的製作流程，以及最終以什麼規格完成和放映**，通常並沒有得到完整、結構化的保存和展示。

IMDb Tech Manager 希望把這些資訊真正帶進個人影視媒體庫的工作流中。

---
## 🖼️ 界面預覽

### 資料管理

<div align="center">

<img src="../images/data-management.png" alt="IMDb Tech Manager Data Management" width="700">

</div>

</p>

nfo資料管理端用於 IMDb Technical Specifications 獲取、技術規格整理、標籤生成以及批量任務管理。

### NFO 管理

<div align="center">

<img src="../images/tag-management.png" alt="IMDb Tech Manager Tag Management" width="700">

</div>

</p>

Tag 管理功能用於檢查、預演和修改媒體庫中的技術元資料，並允許使用者對自動生成的內容進行手動修正。

### AI Runtime 管理

<div align="center">

<img src="../images/ai-runtime-management.png" alt="IMDb Tech Manager AI Runtime Management" width="700">

</div>

</p>

AI Runtime 管理界面用於配置模型接口、API Base URL、Prompt Cache、推理參數、系統提示詞以及 AI 標籤生成相關行為。

### 媒體庫技術規格展示

<div align="center">

<img src="../images/media-library-card.png" alt="IMDb Tech Manager Media Library Technical Specifications" width="900">

</div>

</p>

經過處理的技術規格可以進一步用於媒體庫中的技術資訊展示。

---

## 🔄 核心工作流

```text
IMDb Technical Specifications
        ↓
資料獲取與結構化
        ↓
技術規格標準化
        ↓
NFO 元資料管理
        ↓
技術標籤生成
        ↓
媒體庫中的技術資訊展示
```

項目的目標並不是簡單保存 IMDb 頁面上的原始文字，而是希望把這些技術規格轉化為可以：

* 管理
* 標準化
* 手動修正
* 搜索
* 標籤化
* 展示
* 被其他工具繼續利用

的結構化資訊。

---
## 📚 主要技術資訊

IMDb Technical Specifications 中包含大量影視製作技術資料，例如：

* 📷 攝影機（Cameras）
* 🔭 攝影鏡頭（Lenses）
* 🎞️ 膠片採集格式
* 💾 數字採集格式
* 🎥 攝影工藝（Cinematographic Process）
* 🧪 實驗室與後期製作流程
* 🖼️ 畫幅比例（Aspect Ratio）
* 🔊 聲音格式（Sound Mix）
* 📽️ 母版與放映格式
* 🎬 Printed Film Format
* 其他與製作相關的 Technical Specifications

IMDb Tech Manager 將圍繞這些資料建立解析、標準化、元資料管理和展示能力。

---

## 🧩 項目架構

IMDb Tech Manager（ITM）與 [Tech Card Manager（TCM）](https://github.com/Eric-Hou1997/Tech-Card-Manager) 圍繞同一套 **Technical Specifications** 工作流協同工作。

兩者分別負責不同階段：

* **ITM** 負責技術規格的獲取、處理、檢查與元資料維護
* **TCM** 負責將已經處理好的技術規格應用到媒體庫（如在Emby的界面中生成相應的技術規格卡片），並完成展示與集成

兩者是相互配合的獨立工具，也可以根據實際需求分別使用和獨立發展。

### 📦 IMDb Tech Manager（ITM）

**IMDb Tech Manager（ITM）** 主要負責 Technical Specifications 的資料管理與處理。

主要職責包括：

* IMDb Technical Specifications 資料獲取
* Technical Specifications 結構化
* 技術規格標準化
* NFO 檔案管理
* Technical Specifications 寫入 NFO
* 技術標籤生成
* AI 輔助語義處理
* Preview / Dry Run
* 手動修正
* 批量處理
* 元資料維護

ITM 負責將原始技術規格整理成穩定、結構化、可維護的媒體元資料。

這些經過 ITM 處理後的 Technical Specifications，可以進一步交由 **TCM** 使用，在媒體庫中完成技術規格卡片生成、展示與同步。

### 🖥️ [Tech Card Manager（TCM）](https://github.com/Eric-Hou1997/Tech-Card-Manager)

**Tech Card Manager（TCM）** 主要負責 Technical Specifications 在媒體庫中的展示與集成。

主要職責包括：

* 讀取 ITM 或其他兼容資料源提供的 Technical Specifications
* 技術規格卡片生成與維護
* 技術資訊展示
* 元資料同步
* Web UI 集成
* 不同媒體類型的兼容處理
* 與媒體庫元資料工作流聯動

TCM 與 ITM 使用相同的 Technical Specifications 資料體系，但職責有所區分。

ITM 更關注技術資訊本身的獲取、整理和維護，TCM 則負責將這些已經整理好的資料應用到實際媒體庫環境中。

因此，兩者可以形成一套完整的工作流：

**IMDb → ITM → NFO / Technical Specifications → TCM → 媒體庫展示**

### 🧭 平台關係

ITM 與 TCM 的職責劃分**不與某一個操作系統或媒體服務器永久綁定**。

目前實際已經實現或正在重點開發的是：

* **ITM**：當前主要運行於 **macOS** 的 Technical Specifications 資料管理工具
* **TCM**：當前主要圍繞 **Emby** 開發的 Technical Specifications 卡片管理與媒體庫集成工具

這些只是現階段的實現形態，並不代表兩個項目未來只能運行在這些平台上。

後續可以繼續擴展：

* 其他操作系統
* 其他媒體服務器
* 其他部署方式
* 其他客戶端
* 其他兼容的 Technical Specifications 資料源

ITM 與 TCM 在架構上保持相對獨立，通過標準化的 Technical Specifications 與媒體元資料進行銜接，從而為後續擴展不同平台和媒體庫提供空間。

## 🧠 項目設計原則

IMDb Tech Manager 的開發重點之一，是讓自動化能力保持**可控、可檢查、可回退**。

無論使用本地規則還是 AI，最終目標都是可靠地維護使用者的媒體元資料，而不是單純追求自動化程度。

### 1. 確定性規則優先

對於可以通過明確規則穩定解決的問題，優先使用本地確定性邏輯。

例如：

* 已知格式映射
* 技術規格標準化
* 標籤規則
* 資料結構轉換
* NFO 讀取與寫入
* 檔案處理
* 資料驗證
* 所有權判斷

這類任務具有明確輸入和明確結果，使用確定性規則通常更容易測試、復現和驗證。

不會為了使用 AI，而把能夠可靠解決的問題交給模型處理。

### 2. AI 處理模糊語義

AI 主要用於固定規則難以完整覆蓋的語義問題，例如：

* 不規則自然語言
* 有歧義的技術規格
* 複雜語義拆分
* 廠商、系列與型號關係判斷
* 多種表達方式的歸一化
* 需要結合上下文理解的技術資訊

AI 是 IMDb Tech Manager 的一個能力模塊。

它負責補充規則系統不擅長的部分，而不會取代整個資料處理流程。

### 3. AI Provider 可替換

模型層保持可配置，當前主要通過類似下面的配置接入不同模型服務：

```text
Provider
Base URL
API Key
Model
```

這樣可以根據：

* 模型能力
* 調用成本
* 可用性
* API 兼容性
* 部署環境
* 隱私要求

切換不同的模型服務。

項目不會把核心資料工作流永久綁定到某一個 AI Provider。

### 4. 修改前先預演

涉及 NFO 和媒體元資料修改的操作，盡可能先提供可檢查的結果，再執行正式寫入。

典型流程：

```text
Preview / Dry Run
        ↓
檢查修改結果
        ↓
正式執行
        ↓
驗證寫入結果
```

尤其適用於：

* NFO 修改
* Technical Specifications 寫入
* 技術標籤生成
* 技術標籤更新
* 批量操作
* 元資料遷移

對於批量修改，能夠提前看到“將要發生什麼”比單純提高執行速度更重要。

### 5. 尊重使用者維護的資料

IMDb Tech Manager 會區分不同來源和所有權的元資料。

自動化流程只能修改自己能夠明確確認所有權的資料，不應該因為文本相似，就推斷某個標籤屬於 IMDb Tech Manager。

特別是：

* 使用者手動維護的標籤
* 外部工具生成的標籤
* TMM 等其他應用維護的資料
* 無法確認來源的資料

都不應該被後台任務擅自刪除、接管或覆蓋。

使用者手動修改過的 Generated Tech Tag 會被視為需要保護的人工資料。

### 6. Technical Specifications 與 Tags 分層

項目將 **Technical Specifications** 視為事實資料層，將 **Tags** 視為基於事實資料進一步生成的派生資訊。

```text
Technical Specifications
        ↓
標準化 / 語義處理
        ↓
Technical Tags
```

因此：

* Spec Agent 只負責 Technical Specifications
* 標籤生成器負責 Technical Tags
* 修改 Technical Specifications 不應該隱式觸發 IMDb 資料刷新
* 標籤重建應該是獨立、可觀察的操作

這樣可以避免不同處理階段互相污染，也方便後續重新生成標籤或接入其他工具。

### 7. 寫入操作必須安全

NFO 是媒體庫的重要長期資料，因此修改流程需要盡量避免“寫了一半”“誤刪標籤”或“檔案損壞”這類問題。

項目會持續強化：

* XML 有效性檢查
* 修改前備份
* 原檔案狀態檢查
* 原子寫入
* 路徑安全檢查
* 標籤所有權驗證
* 衝突檢測
* 失敗時保持原檔案不變
* 可恢復與可撤銷能力

當系統無法安全判斷是否應該修改某項資料時，預設選擇跳過，而不是冒險寫入。

### 8. 後台行為可觀察、可驗證

重要後台操作應該盡可能向使用者提供明確狀態。

例如：

* 正在處理什麼
* 修改了什麼
* 哪些結果來自本地規則
* 哪些結果來自 AI
* API 調用次數
* Token 使用情況
* Cache 命中情況
* 估算費用
* 當前任務階段
* 成功 / 失敗 / 跳過狀態
* 錯誤原因
* 受影響的影片或 NFO

項目盡量避免只顯示一個簡單的“成功”，而忽略中間實際發生了什麼。

---

## 🤖 面向 Coding Agent 的開發

IMDb Tech Manager 的公開 Repository 已經提供：

[**`AGENTS.md` →**](../../AGENTS.md)

它是 Codex 等 Coding Agent 理解和修改項目的重要上下文入口。

`AGENTS.md` 目前已經包含：

* Repository 身份與源碼邊界
* 當前產品職責
* 軟體架構約束
* NFO 安全規則
* 標籤所有權規則
* AI 調用與 Token 統計規則
* 生命週期管理要求
* 測試與驗證要求
* Release 邊界
* OTA 簽名要求
* 平台支持邊界
* 不應該進行的修改
* 修改完成前需要執行的檢查

開發者 Fork 或 Clone 項目後，可以讓支持 Repository Context 的 Coding Agent 先讀取 `AGENTS.md`，再開始分析和修改代碼。

推薦工作流：

```text
Fork / Clone
        ↓
Coding Agent 讀取 AGENTS.md
        ↓
讀取相關源碼與測試
        ↓
理解模塊職責與修改邊界
        ↓
分析受到影響的功能鏈路
        ↓
制定修改方案
        ↓
修改代碼
        ↓
運行相關測試
        ↓
驗證真實行為
        ↓
提交 Pull Request
```

IMDb Tech Manager 希望 Repository 提供的不只是源碼本身：

```text
源代碼
  +
架構知識
  +
設計約束
  +
開發規則
  +
測試方法
  +
Agent Context
```

這樣無論是開發者直接閱讀代碼，還是使用 Codex 等 Coding Agent，都可以更快理解項目，同時減少“代碼能運行，但破壞了既有設計”的修改。

---

## 🚧 當前狀態

IMDb Tech Manager 目前處於 **公開源碼、持續開發階段**。

公開源碼從 **v4.0.0** 開始維護。

當前 Repository 已經包含：

* 完整項目源碼
* macOS 當前實現
* `AGENTS.md`
* 測試代碼
* Release 構建工具
* Packaging 配置
* 項目文檔
* `LICENSE`
* `NOTICE`
* `SECURITY.md`
* [`PRIVACY.md`](../legal/PRIVACY.zh-Hant.md)
* [`TERMS.md`](../legal/TERMS.zh-Hant.md)

### 當前支持平台

目前正式維護的實現為：

**macOS · Apple Silicon（arm64）**

當前桌面端主要由以下部分組成：

```text
Native Launcher
      +
Go Core
      +
Local Web UI
      +
Python Engine
```

Repository 按產品組織，而不是永久按照操作系統劃分。

未來如果擴展 Windows 或其他平台，仍然屬於 IMDb Tech Manager，只要保持相同的產品職責和資料工作流。

### 當前正式版本

當前公開版本：

**IMDb Tech Manager v4.1.0**

Release 已提供 Apple Silicon `.app` 的 ZIP 發佈包，並同時提供：

* Release Changelog
* SHA-256 校驗資訊
* OTA Ed25519 簽名
* 安裝說明

[**查看 Releases →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)

應用內置簡體中文、繁體中文和 English (United States)。法語、俄語、日語、西班牙語與泰語通過對應應用版本的 GitHub Release 語言包提供，下載並驗證後加載。詳情見 [`docs/language-packs.md`](../language-packs.md)。

項目仍然處於持續開發階段，功能、架構、測試覆蓋和平台支持都會繼續迭代。

---

## 🗺️ Roadmap

### 已完成

* [x] 建立公開 Repository
* [x] 開放核心源碼
* [x] 建立項目基礎目錄結構
* [x] 發佈 `AGENTS.md`
* [x] 確定開源許可證
* [x] 建立基礎測試體系
* [x] 建立 Release 構建流程
* [x] 發佈首個公開源碼版本 `v4.0.0`
* [x] 建立 macOS Apple Silicon 發佈流程
* [x] 加入 Release 完整性校驗與 OTA 簽名
* [x] 發佈 `v4.1.0`，內置簡體中文、繁體中文與 English (United States)
* [x] 發佈法語、俄語、日語、西班牙語和泰語獨立語言包
* [x] 讓 Web、Go Core、Python Engine 與 macOS 原生菜單共享語言狀態
* [x] 固定任務啓動時的日誌與復核語言，並保留舊日誌、緩存和提示詞
* [x] 區分代理/網路、GitHub 限流、資產缺失、下載與簽名驗證錯誤

### 持續推進

* [ ] 完善 Technical Specifications 標準化規則
* [ ] 擴展更多 Technical Specifications 資料處理能力
* [ ] 完善 Local / AI 技術標籤生成
* [ ] 強化 NFO 資料所有權與安全機制
* [ ] 擴展自動化測試與真實場景回歸測試
* [ ] 改進任務狀態、錯誤定位與恢復能力
* [ ] 完善 AI Provider 與模型配置能力
* [ ] 優化 Token、Cache 與調用成本統計
* [ ] 完善應用更新與 Release 工作流
* [ ] 完善 Developer ID 簽名與 macOS 分發體驗
* [ ] 持續完善 `AGENTS.md` 與 Coding Agent Context
* [ ] 完善貢獻與 Pull Request 工作流
* [ ] 探索其他操作系統支持
* [ ] 與 [Tech Card Manager（TCM）](https://github.com/Eric-Hou1997/Tech-Card-Manager) 完善 Technical Specifications 工作流銜接
* [ ] 探索更多媒體服務器與客戶端的集成方式

Roadmap 會隨著項目開發和實際使用反饋持續調整。

---

## 💬 Discussions

功能想法、技術方案、UI 設計、Technical Specifications 標準化規則以及開發工作流，都歡迎在 Discussions 中交流：

[**進入 GitHub Discussions →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/discussions)

適合 Discussions 的內容包括：

* 新功能想法
* 技術方案討論
* Technical Specifications 資料規則
* 攝影機 / 鏡頭 / 膠片 / 製作格式資料
* UI / UX 建議
* AI 標籤生成策略
* ITM 與 TCM 的工作流
* Coding Agent 開發方式
* 尚未完全確定的問題

如果一個想法還需要進一步討論和驗證，可以先放到 Discussions，而不必立即整理成 Issue。

---

## 🐛 Issues

對於已經能夠明確描述的問題，可以直接提交 Issue：

[**GitHub Issues →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/issues)

例如：

* 可以穩定復現的 Bug
* 明確的功能缺失
* 資料解析錯誤
* Technical Specifications 標準化錯誤
* NFO 修改異常
* UI 行為異常
* Release / 安裝問題
* 已經比較清晰的功能需求

提交問題時，如果條件允許，建議同時提供相關版本、媒體類型、操作步驟和錯誤資訊，以便更快定位問題。

---

## 🤝 Contributing

IMDb Tech Manager 已經開放源碼，歡迎 Fork、研究、修改並提交 Pull Request。

開始修改代碼之前，建議先閱讀：

[**`AGENTS.md` →**](../../AGENTS.md)

它記錄了項目當前的重要架構規則、資料安全約束、測試要求和 Release 邊界。

尤其是在修改以下模塊時，請先瞭解現有設計：

* NFO 讀寫
* Technical Specifications
* Technical Tags
* 標籤所有權
* AI 調用
* Token / Cache 統計
* 批量任務
* 應用生命週期
* 平台相關代碼
* 更新與 Release

項目歡迎包括但不限於以下類型的貢獻：

* Bug 修復
* 功能改進
* Technical Specifications 解析規則
* 技術規格標準化
* 攝影機 / 鏡頭 / 製作格式資料
* 測試用例
* UI / UX 改進
* 性能與穩定性改進
* 文檔完善
* Coding Agent Context 改進

在修改已有行為時，請盡量補充對應的測試或回歸驗證，避免修復一個問題的同時破壞已有媒體資料工作流。

---

## 📄 License

IMDb Tech Manager 採用 **Apache License 2.0** 開源。

完整許可證請查看：

[**LICENSE →**](../../LICENSE)

Repository 同時提供：

[**NOTICE →**](../../NOTICE)

使用、修改和分發源碼時，請遵守 Apache License 2.0 以及 Repository 中相關說明。

---

## ⚠️ 免責聲明

IMDb Tech Manager 是一個獨立開發的開源項目。

本項目**與 IMDb、Emby 以及其他第三方平台不存在官方隸屬、授權或背書關係**。

相關第三方名稱、商標、資料和服務歸各自權利方所有。

IMDb Tech Manager 提供的是技術規格獲取、處理和媒體元資料管理工具。

使用者在使用第三方資料、API、網站或服務時，應自行確認相關使用方式符合適用的服務條款、授權條件和法律要求。

---

## 💡 反饋與建議

IMDb Tech Manager 仍在持續開發。

如果你對以下方向有想法：

* IMDb Technical Specifications
* 技術規格標準化
* 攝影機與鏡頭資料
* 膠片與數字採集格式
* NFO 元資料管理
* 技術標籤規則
* AI 輔助語義處理
* UI / UX
* ITM 與 TCM 的協作方式
* 其他操作系統支持
* Coding Agent 開發工作流

歡迎通過 Discussions 或 Issues 參與項目。

項目會根據實際使用反饋繼續調整功能、資料規則和開發方向。
