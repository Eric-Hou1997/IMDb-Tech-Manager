<div align="center">

# IMDb Tech Manager

[簡體中文](../../README.md) | **繁體中文** | [English](./README.en.md) | [Français](./README.fr.md) | [Русский](./README.ru.md) | [日本語](./README.ja.md) | [Español](./README.es.md) | [ไทย](./README.th.md)

[![Release](https://img.shields.io/github/v/release/Eric-Hou1997/IMDb-Tech-Manager?label=release)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Downloads](https://img.shields.io/github/downloads/Eric-Hou1997/IMDb-Tech-Manager/total?label=downloads)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)

<img src="../../macos/assets/ITM_logo.png" alt="IMDb Tech Manager" width="220">

影視技術規格與媒體庫中繼資料管理工具。

</div>

## 專案簡介

IMDb Tech Manager（ITM）取得並結構化 IMDb Technical Specifications，將攝影機、鏡頭、拍攝格式、聲音格式、畫面比例與製作流程等資料安全地寫入 NFO。它也提供 Inspector、預演、撤銷、批次任務，以及本機規則或 AI 輔助的技術標籤管理。

完整工作流程可以與 [Tech Card Manager（TCM）](https://github.com/Eric-Hou1997/Tech-Card-Manager) 搭配：

```text
IMDb → ITM → NFO / Technical Specifications → TCM → Emby 顯示
```

## 目前版本與平台

- 正式版本：[`v4.1.0`](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases/tag/v4.1.0)
- 平台：macOS 12 或以上，Apple Silicon（arm64）
- 安裝包：`ITM-v4.1.0-MacOS-AArch64-APP.zip`
- ZIP 內的應用名稱固定為 `IMDb Tech Manager.app`
- Release 同時提供 SHA-256、Ed25519 OTA 簽章、說明與更新記錄

## 核心能力

- 取得、快取並結構化 IMDb Technical Specifications
- 僅在 `<technicalspecs>` 範圍內安全寫入 NFO
- Inspector、修改前預演、來源雜湊檢查、備份與撤銷
- 區分 External、Generated 與 Manual 技術標籤的權屬
- 電影與電視劇獨立搜尋、篩選、選取與批次任務
- 可替換的 OpenAI-compatible / Anthropic AI Provider
- 真實 HTTP 次數、Token、快取與費用統計
- 可區分代理、限流、資產缺失、下載及簽章錯誤的 OTA 更新

## 語言

簡體中文、繁體中文與 English (United States) 內建於應用程式。Français、Русский、日本語、Español 與ไทย以 `v4.1.0` Release 的獨立語言包提供，下載並驗證後載入。

Web UI、Go Core、Python Engine 與 macOS 原生選單共用語言狀態。任務開始時會固定日誌與複核語言；切換介面不會改寫舊日誌、快取、NFO、權屬資訊、預設系統提示詞或自訂提示詞。

## 安全邊界

- Spec Agent 只能修改 `<technicalspecs>`，不能新增或刪除根層級 `<tag>`。
- 自動流程只能處理由本產品可靠擁有的 Generated Tech Tags。
- External、Manual、TMM 或其他應用的標籤預設保持不變。
- XML、UTF-8 BOM、換行、檔案模式、備份與原子替換都會保留或驗證。
- 路徑不明、權屬衝突或來源在預演後改變時會安全停止。

## 介面預覽

![資料管理](../images/data-management.png)

![標籤管理](../images/tag-management.png)

## Roadmap

已完成：Apple Silicon 發布流程、NFO 安全與權屬模型、Local/AI 標籤流程、任務與用量管理、雙語原生介面、五種下載語言包、版本綁定更新與完整回歸門禁。

持續推進：Technical Specifications 標準化、更多真實媒體庫回歸、AI Provider 相容性、錯誤恢復、Developer ID 簽署與 notarization，以及其他平台與媒體伺服器整合。

## 開發與授權

開發前請閱讀 [`AGENTS.md`](../../AGENTS.md)。語言包設計見 [`docs/language-packs.md`](../language-packs.md)。

本專案採用 [Apache License 2.0](../../LICENSE)。IMDb、Emby 及其他商標歸各自權利人所有；本專案與 IMDb.com, Inc. 或 Emby LLC 無從屬、授權或背書關係。
