<div align="center">

# IMDb Tech Manager

[简体中文](./README.md) | [繁體中文](./README.zh-Hant.md) | [English](./README.en.md) | [Français](./README.fr.md) | [Русский](./README.ru.md) | **日本語** | [Español](./README.es.md) | [ไทย](./README.th.md)

[![Release](https://img.shields.io/github/v/release/Eric-Hou1997/IMDb-Tech-Manager?label=release)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Downloads](https://img.shields.io/github/downloads/Eric-Hou1997/IMDb-Tech-Manager/total?label=downloads)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)

<img src="./macos/assets/ITM_logo.png" alt="IMDb Tech Manager" width="220">

映像作品の技術仕様とメディアライブラリのメタデータを管理するツールです。

</div>

## 概要

IMDb Tech Manager（ITM）は IMDb Technical Specifications を取得して構造化し、カメラ、レンズ、撮影形式、音声形式、アスペクト比、制作工程などの情報を安全に NFO へ保存します。Inspector、書き込み前のプレビュー、取り消し、バッチタスク、ローカルルールまたは AI による技術タグ管理も備えています。

[Tech Card Manager（TCM）](https://github.com/Eric-Hou1997/Tech-Card-Manager) と組み合わせると、次の流れを構成できます。

```text
IMDb → ITM → NFO / Technical Specifications → TCM → Emby で表示
```

## 現在のバージョンと対応環境

- 安定版：[`v4.1.0`](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases/tag/v4.1.0)
- 対応環境：macOS 12 以降、Apple Silicon（arm64）
- 配布ファイル：`ITM-v4.1.0-MacOS-AArch64-APP.zip`
- ZIP 内のアプリ名：`IMDb Tech Manager.app`
- SHA-256、Ed25519 OTA 署名、説明書、変更履歴を同梱しています

## 主な機能

- IMDb Technical Specifications の取得、キャッシュ、構造化
- `<technicalspecs>` のみに限定した安全な NFO 書き込み
- Inspector、プレビュー、ソースハッシュ検証、バックアップ、取り消し
- External／Generated／Manual 技術タグの所有権を明確に区別
- 映画とテレビ番組ごとの検索、フィルター、選択、バッチ処理
- OpenAI-compatible／Anthropic の交換可能な AI Provider
- 実際の HTTP 回数、Token、キャッシュ、費用の集計
- プロキシ、GitHub 制限、アセット欠落、ダウンロード、署名エラーを区別する OTA

## 言語

簡体字中国語、繁体字中国語、英語（米国）はアプリに内蔵されています。フランス語、ロシア語、日本語、スペイン語、タイ語は `v4.1.0` Release の個別言語パックとして提供され、ダウンロードと検証の後に読み込まれます。

Web UI、Go Core、Python Engine、macOS ネイティブメニューは同じ言語状態を共有します。ログとレビュー説明の言語はタスク開始時に固定されます。UI を切り替えても、過去のログ、キャッシュ、NFO、所有権情報、既定またはカスタムのプロンプトは書き換えられません。

## 安全境界

- Spec Agent が変更できるのは `<technicalspecs>` だけで、ルート `<tag>` は変更しません。
- 自動処理は、本製品の所有が証明された Generated Tech Tags だけを対象にします。
- External、Manual、TMM、他アプリのタグは維持されます。
- XML、UTF-8 BOM、改行、ファイルモード、バックアップ、アトミック置換を保持または検証します。
- パスや所有権が不明な場合、またはプレビュー後にファイルが変わった場合は安全に停止します。

## 画面

![データ管理](./docs/images/data-management.png)

![タグ管理](./docs/images/tag-management.png)

## ロードマップ

完了：Apple Silicon 配布、NFO の安全性と所有権、Local/AI タグ、タスクと使用量の管理、3 言語内蔵 UI、5 種類のダウンロード言語、バージョン連動更新、回帰テストゲート。

継続：Technical Specifications の標準化、実ライブラリでの検証、AI Provider の互換性、エラー回復、Developer ID 署名と notarization、他のプラットフォームやメディアサーバーとの統合。

## 開発とライセンス

開発前に [`AGENTS.md`](./AGENTS.md) を確認してください。言語パックの設計は [`docs/language-packs.md`](./docs/language-packs.md) にあります。

本プロジェクトは [Apache License 2.0](./LICENSE) で公開されています。IMDb、Emby などの商標は各所有者に帰属します。本プロジェクトは IMDb.com, Inc. または Emby LLC と提携、承認、推奨の関係にはありません。
