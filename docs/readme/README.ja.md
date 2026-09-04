<div align="center">

# IMDb Tech Manager

[简体中文](../../README.md) | [繁體中文](./README.zh-Hant.md) | [English](./README.en.md) | [Français](./README.fr.md) | [Русский](./README.ru.md) | **日本語** | [Español](./README.es.md) | [ไทย](./README.th.md)

<p align="center">

[![リリース](https://img.shields.io/github/v/release/Eric-Hou1997/IMDb-Tech-Manager?label=release)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![ダウンロード](https://img.shields.io/github/downloads/Eric-Hou1997/IMDb-Tech-Manager/total?label=downloads)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![スター](https://img.shields.io/github/stars/Eric-Hou1997/IMDb-Tech-Manager?style=flat\&logo=github)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/stargazers)
[![PR 歓迎](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/pulls)

</p>

<img src="../../macos/assets/ITM_logo.png" alt="IMDb Tech Manager" width="220">

映画・メディアライブラリ向けの技術仕様およびメタデータ管理。

---

## 🎬 概要

**IMDb Tech Manager** は、映画およびテレビ作品の技術情報を管理するためのプロジェクトです。

</p>

<img src="../images/poster.jpg" alt="IMDb Tech Manager ポスター" width="700">

</div>

</p>

本プロジェクトは **IMDb Technical Specifications** の取得、構造化、正規化、適用に重点を置き、散在する映画・テレビ制作情報を、個人のメディアライブラリで管理、検索、表示できるメタデータへ変換します。

現在の主な対象分野は次のとおりです。

* IMDb Technical Specifications
* NFO メタデータ管理
* 技術タグ生成
* カメラおよびレンズ情報
* フィルムおよびデジタル撮影形式
* 制作および上映形式
* 技術メタデータの正規化
* メディアライブラリでの技術情報表示
* AI 支援による意味処理
* Coding Agent 支援による開発

多くのメディアライブラリは、タイトル、出演者、公開年、解像度、コーデック、音声形式などをすでに優れた形で提供しています。

しかし、**どのカメラやレンズが使われたか、どのフィルムまたはデジタル撮影形式が関係したか、どのような制作工程が用いられたか、最終作品がどのようにマスタリング・上映されたか**といった情報が、完全かつ構造化された形で保存・表示されることはほとんどありません。

IMDb Tech Manager は、この情報を個人のメディアライブラリのワークフローに取り込むことを目指します。

---

## 🖼️ スクリーンショット

### データ管理

<div align="center">

<img src="../images/data-management.png" alt="IMDb Tech Manager データ管理" width="700">

</div>

</p>

NFO データ管理画面では、IMDb Technical Specifications の取得、技術仕様の整理、タグ生成、バッチタスク管理を行います。

### NFO 管理

<div align="center">

<img src="../images/tag-management.png" alt="IMDb Tech Manager タグ管理" width="700">

</div>

</p>

タグ管理では、メディアライブラリ内の技術メタデータを検査、プレビュー、変更でき、自動生成された内容を手動で修正できます。

### AI ランタイム管理

<div align="center">

<img src="../images/ai-runtime-management.png" alt="IMDb Tech Manager AI ランタイム管理" width="700">

</div>

</p>

AI ランタイム管理画面では、モデルのエンドポイント、API Base URL、Prompt Cache、推論パラメータ、システムプロンプトなど、AI タグ生成に関係する動作を設定します。

### メディアライブラリの技術仕様

<div align="center">

<img src="../images/media-library-card.png" alt="IMDb Tech Manager メディアライブラリ技術仕様" width="900">

</div>

</p>

処理済みの技術仕様は、メディアライブラリ内で技術情報を表示するために利用できます。

---

## 🔄 基本ワークフロー

```text
IMDb Technical Specifications
        ↓
データの取得と構造化
        ↓
技術仕様の正規化
        ↓
NFO メタデータ管理
        ↓
技術タグ生成
        ↓
メディアライブラリでの技術情報表示
```

目的は IMDb ページの生テキストを単に保存することではありません。技術仕様を次のように扱える構造化情報へ変換することです。

* 管理
* 正規化
* 手動修正
* 検索
* タグ付け
* 表示
* 他のツールで再利用

---

## 📚 技術情報

IMDb Technical Specifications には、映画およびテレビ制作に関する幅広い情報が含まれます。

* 📷 カメラ
* 🔭 レンズ
* 🎞️ フィルム撮影形式
* 💾 デジタル撮影形式
* 🎥 撮影プロセス
* 🧪 ラボおよびポストプロダクション工程
* 🖼️ アスペクト比
* 🔊 サウンドミックス
* 📽️ マスターおよび上映形式
* 🎬 プリントフィルム形式
* その他の制作関連 Technical Specifications

IMDb Tech Manager は、この情報を対象に解析、正規化、メタデータ管理、表示機能を構築します。

---

## 🧩 プロジェクト構成

IMDb Tech Manager (ITM) と [Tech Card Manager (TCM)](https://github.com/Eric-Hou1997/Tech-Card-Manager) は、同じ **Technical Specifications** ワークフロー上で連携します。

2 つのツールは異なる段階を担当します。

* **ITM** は技術仕様および関連メタデータの取得、処理、検査、保守を担当します。
* **TCM** は処理済みの技術仕様をメディアライブラリへ適用し、たとえば Emby 画面に対応する Technical Specifications カードを生成して、表示と統合を担当します。

ITM と TCM は連携するよう設計された独立ツールであり、ワークフローに応じて個別に使用・開発することもできます。

### 📦 IMDb Tech Manager (ITM)

**IMDb Tech Manager (ITM)** は主に Technical Specifications データの管理と処理を担当します。

担当範囲は次のとおりです。

* IMDb Technical Specifications の取得
* Technical Specifications の構造化
* 技術仕様の正規化
* NFO ファイル管理
* NFO ファイルへの Technical Specifications 書き込み
* 技術タグ生成
* AI 支援による意味処理
* プレビュー / ドライラン
* 手動修正
* バッチ処理
* メタデータ保守

ITM は未加工の技術仕様を、安定して構造化され、保守可能なメディアメタデータへ変換します。

ITM が処理した Technical Specifications は、**TCM** がメディアライブラリ内で技術仕様カードを生成、表示、同期するために利用できます。

### 🖥️ [Tech Card Manager (TCM)](https://github.com/Eric-Hou1997/Tech-Card-Manager)

**Tech Card Manager (TCM)** は主にメディアライブラリ内での Technical Specifications の表示と統合を担当します。

担当範囲は次のとおりです。

* ITM または他の互換データソースが提供する Technical Specifications の読み取り
* 技術仕様カードの生成と保守
* 技術情報の表示
* メタデータの同期
* Web UI 統合
* 異なるメディア種別の互換性処理
* メディアライブラリのメタデータワークフローとの統合

TCM と ITM は同じ Technical Specifications データモデルを使用しながら、それぞれ異なる責務に集中します。

ITM は技術情報そのものを取得、整理、保守し、TCM は処理済みデータを実際のメディアライブラリ環境へ適用します。

両者を組み合わせると、完全なワークフローを構成できます。

**IMDb → ITM → NFO / Technical Specifications → TCM → メディアライブラリ表示**

### 🧭 プラットフォームとの関係

ITM と TCM の責務は、**特定のオペレーティングシステムやメディアサーバーに恒久的に結び付くものではありません**。

現在利用可能、または活発に開発中の実装は次のとおりです。

* **ITM**：現在 **macOS** を中心とする Technical Specifications データ管理アプリケーション
* **TCM**：現在 **Emby** を中心とする Technical Specifications カード管理およびメディアライブラリ統合ツール

これらは現在の実装にすぎず、どちらのプロジェクトもこれらのプラットフォームだけに限定されることを意味しません。

将来は次のような拡張が考えられます。

* 他のオペレーティングシステム
* 他のメディアサーバー
* 他の導入方法
* 他のクライアントアプリケーション
* 他の互換 Technical Specifications データソース

ITM と TCM はアーキテクチャ上の相対的な独立性を維持し、標準化された Technical Specifications とメディアメタデータを通じて連携するため、今後さまざまなプラットフォームやメディアライブラリ環境へ拡張できます。

## 🧠 設計原則

IMDb Tech Manager の中核となる開発目標の 1 つは、自動化を **制御可能、検査可能、復旧可能** に保つことです。

タスクが決定論的なローカルルールを使用する場合でも AI を使用する場合でも、単に自動化を最大化するのではなく、ユーザーのメディアメタデータを確実に保守することを目指します。

### 1. 決定論的ルールを優先する

明示的なルールで確実に解決できる問題には、決定論的なローカルロジックを優先します。

例：

* 既知の形式マッピング
* 技術仕様の正規化
* タグルール
* データ構造の変換
* NFO の読み書き
* ファイル処理
* データ検証
* 所有権判定

これらのタスクには明確な入力と出力があるため、決定論的ロジックの方がテスト、再現、検証を容易に行えます。

ルールで確実に解決できるタスクを、AI を使うこと自体を目的として AI に渡すことはありません。

### 2. 曖昧な意味処理に AI を使う

AI は主に、固定ルールでは完全に網羅しにくい次のような意味上の問題に使用します。

* 不規則な自然言語の説明
* 曖昧な技術仕様
* 複雑な意味分解
* メーカー、シリーズ、モデル間の関係特定
* 同じ情報を表す複数の表現の正規化
* 文脈に基づく解釈が必要な技術情報

AI は IMDb Tech Manager が持つ機能の 1 つです。

決定論的ルールが効果を発揮しにくい領域を補完しますが、データ処理パイプライン全体を置き換えるものではありません。

### 3. AI プロバイダーを交換可能にする

モデル層は設定可能な状態を保ち、次のような設定を通じて異なるモデルサービスへ接続できます。

```text
プロバイダー
ベース URL
API キー
モデル
```

ユーザーは次の条件に応じてモデルサービスを切り替えられます。

* モデルの能力
* API コスト
* 可用性
* API 互換性
* 導入環境
* プライバシー要件

中核となるデータワークフローが、特定の AI プロバイダーへ恒久的に依存することはありません。

### 4. 書き込み前にプレビューする

NFO ファイルやメディアメタデータを変更する操作は、可能な限り実際に書き込む前に検査可能な結果を提示します。

一般的なワークフロー：

```text
プレビュー / ドライラン
        ↓
変更を確認
        ↓
実行
        ↓
結果を検証
```

これは特に次の操作で重要です。

* NFO の変更
* Technical Specifications の書き込み
* 技術タグ生成
* 技術タグ更新
* バッチ操作
* メタデータ移行

バッチ操作では、単に実行速度を最大化するより、何が変更されるかを事前に把握することが重要です。

### 5. ユーザーが管理するデータを尊重する

IMDb Tech Manager はメタデータを出所と所有権によって区別します。

自動処理が変更できるのは、アプリケーションが自らのデータだと確実に識別できるものだけです。テキストが似ているだけで、タグが IMDb Tech Manager の所有物だと判断してはなりません。

特に次のデータは、バックグラウンドタスクによって暗黙に削除、取得、上書きしてはなりません。

* ユーザーが手動で管理するタグ
* 外部ツールが生成したタグ
* TMM などのアプリケーションが管理するデータ
* 所有権が不明なデータ

ユーザーが手動編集した Generated Tech Tag は、保護対象のユーザー管理データとして扱います。

### 6. Technical Specifications とタグを分離する

本プロジェクトでは、**Technical Specifications** を事実データ層、**タグ** をその事実層から生成される派生情報として扱います。

```text
Technical Specifications
        ↓
正規化 / 意味処理
        ↓
技術タグ
```

したがって：

* Spec Agent は Technical Specifications のみを担当します
* タグジェネレーターは Technical Tags を担当します
* Technical Specifications の編集によって IMDb 更新を暗黙に開始してはなりません
* タグの再構築は独立した観察可能な操作でなければなりません

この分離により各処理段階の干渉を防ぎ、後からタグを再生成したり別のツールを統合したりしやすくなります。

### 7. 安全に書き込む

NFO ファイルはメディアライブラリの重要な長期データであるため、変更ワークフローでは部分書き込み、誤ったタグ削除、ファイル破損の危険を最小化する必要があります。

本プロジェクトは次の項目を継続的に強化します。

* XML 妥当性チェック
* 変更前のバックアップ
* 元ファイルの状態チェック
* アトミック書き込み
* パス安全性の検証
* タグ所有権の検証
* 競合検出
* 失敗時に元ファイルを変更しないこと
* 復旧および取り消し機能

データを変更してよいか安全に判断できない場合は、危険な書き込みを行う代わりに既定で操作をスキップします。

### 8. バックグラウンド動作を観察・検証可能にする

重要なバックグラウンド操作は、可能な限り明確な状態情報を表示します。

例：

* 処理中の対象
* 変更された内容
* ローカルルールによる結果
* AI による結果
* API 呼び出し回数
* トークン使用量
* キャッシュヒット
* 推定コスト
* 現在のタスク段階
* 成功 / 失敗 / スキップ状態
* エラー理由
* 影響を受けたタイトルまたは NFO ファイル

実際に起きたことを示さず、複雑なバックグラウンド作業を単純な「成功」メッセージだけにまとめないことを目指します。

---

## 🤖 Coding Agents を利用した開発

IMDb Tech Manager の公開リポジトリには次のファイルがあります。

[**`AGENTS.md` →**](../../AGENTS.md)

これは Codex などの Coding Agents がプロジェクトを理解・変更するための重要なコンテキスト入口です。

現在の `AGENTS.md` は次の内容を扱います。

* リポジトリの識別情報とソース境界
* 現在の製品責務
* ソフトウェアアーキテクチャの制約
* NFO 安全規則
* タグ所有権の規則
* AI 呼び出しとトークン会計の規則
* ライフサイクル管理要件
* テストおよび検証要件
* リリース境界
* OTA 署名要件
* プラットフォーム対応境界
* 行ってはならない変更
* 変更完了前に必要な確認

リポジトリを Fork または Clone した後、Repository Context に対応する Coding Agent に、コードを分析・変更する前に `AGENTS.md` を読ませることができます。

推奨ワークフロー：

```text
Fork / Clone
        ↓
Coding Agent が AGENTS.md を読む
        ↓
関連するソースコードとテストを読む
        ↓
モジュールの責務と境界を理解する
        ↓
影響を受ける機能経路を分析する
        ↓
実装計画を作成する
        ↓
コードを変更する
        ↓
関連テストを実行する
        ↓
実際の動作を検証する
        ↓
Pull Request を提出する
```

IMDb Tech Manager はソースコードだけでなく、次のものを提供することを目指します。

```text
ソースコード
    +
アーキテクチャ知識
    +
設計上の制約
    +
開発規則
    +
テスト方法
    +
Agent Context
```

これにより、コードを直接読む開発者も Codex などの Coding Agents と作業する開発者も、プロジェクトをより速く理解でき、技術的には動作しても既存の設計制約に反する変更を行うリスクを減らせます。

---

## 🚧 現在の状態

IMDb Tech Manager は現在、**オープンソースで活発に開発中**です。

公開ソースコードの履歴は **v4.0.0** から始まります。

現在のリポジトリには次のものが含まれます。

* プロジェクトの完全なソースコード
* 現在の macOS 実装
* `AGENTS.md`
* テストコード
* リリースビルドツール
* パッケージ設定
* プロジェクト文書
* `LICENSE`
* `NOTICE`
* `SECURITY.md`
* [`PRIVACY.md`](../legal/PRIVACY.ja.md)
* [`TERMS.md`](../legal/TERMS.ja.md)

### 現在対応しているプラットフォーム

現在保守されている実装：

**macOS · Apple Silicon (arm64)**

現在のデスクトップアプリケーションは主に次の要素で構成されます。

```text
ネイティブランチャー
      +
Go Core
      +
ローカル Web UI
      +
Python Engine
```

リポジトリはオペレーティングシステムではなく製品を中心に構成されています。

将来 Windows や他のプラットフォームへ対応する場合も、同じ製品責務とデータワークフローを保つ限り IMDb Tech Manager の一部として扱います。

### 現在の公式リリース

現在の公開リリース：

**IMDb Tech Manager v4.1.0**

リリースでは Apple Silicon 向け `.app` を ZIP パッケージとして提供し、次のファイルを同梱します。

* リリース変更履歴
* SHA-256 検証情報
* OTA Ed25519 署名
* インストール手順

[**リリースを見る →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)

簡体字中国語、繁体字中国語、英語（米国）は内蔵されています。フランス語、ロシア語、日本語、スペイン語、タイ語は、対応するアプリのリリースで検証済み言語パックとして提供されます。詳しくは [`docs/language-packs.md`](../language-packs.md) を参照してください。

本プロジェクトは活発に開発中であり、機能、アーキテクチャ、テスト範囲、プラットフォーム対応は今後も進化します。

---

## 🗺️ ロードマップ

### 完了

* [x] 公開リポジトリの作成
* [x] コアソースコードの公開
* [x] 基本プロジェクト構成の確立
* [x] `AGENTS.md` の公開
* [x] オープンソースライセンスの選定
* [x] 初期テストフレームワークの構築
* [x] リリースビルドワークフローの構築
* [x] 最初の公開ソースリリース `v4.0.0` の公開
* [x] macOS Apple Silicon リリースワークフローの構築
* [x] リリース整合性検証と OTA 署名の追加
* [x] 簡体字中国語、繁体字中国語、英語（米国）を内蔵した `v4.1.0` のリリース
* [x] フランス語、ロシア語、日本語、スペイン語、タイ語の個別言語パック公開
* [x] Web UI、Go Core、Python Engine、macOS ネイティブメニュー間での言語状態共有
* [x] 履歴ログ、キャッシュ、プロンプトを維持しつつ、タスク開始時にログとレビュー言語を固定
* [x] プロキシ/ネットワーク、GitHub 制限、アセット欠落、ダウンロード、署名検証の失敗を区別

### 進行中

* [ ] Technical Specifications 正規化ルールの改善
* [ ] Technical Specifications データ処理機能の拡張
* [ ] ローカル / AI 技術タグ生成の改善
* [ ] NFO データ所有権および安全機構の強化
* [ ] 自動テストと実環境回帰テスト範囲の拡張
* [ ] タスク状態、エラー翻訳、復旧の改善
* [ ] AI プロバイダーおよびモデル設定の改善
* [ ] トークン、キャッシュ、API コスト会計の改善
* [ ] アプリ更新およびリリースワークフローの改善
* [ ] Developer ID 署名と macOS 配布体験の改善
* [ ] `AGENTS.md` と Coding Agent Context の継続的改善
* [ ] コントリビューションと Pull Request ワークフローの改善
* [ ] 他のオペレーティングシステムへの対応検討
* [ ] [Tech Card Manager (TCM)](https://github.com/Eric-Hou1997/Tech-Card-Manager) との Technical Specifications ワークフロー統合改善
* [ ] 他のメディアサーバーおよびクライアントアプリケーションとの統合検討

ロードマップは、プロジェクトの開発状況と実際の利用者の意見に応じて今後も更新されます。

---

## 💬 Discussions

機能案、技術的アプローチ、UI デザイン、Technical Specifications 正規化ルール、開発ワークフローについて、Discussions への投稿を歓迎します。

[**GitHub Discussions へ →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/discussions)

適した話題：

* 新機能のアイデア
* 技術設計の議論
* Technical Specifications のデータ規則
* カメラ / レンズ / フィルム / 制作形式に関する情報
* UI / UX の提案
* AI タグ生成戦略
* ITM と TCM のワークフロー
* Coding Agent を使う開発手法
* さらに検討が必要なアイデア

明確なタスクにする前に議論や検証が必要なアイデアは、Issue を開く前に Discussions から始めるのが適しています。

---

## 🐛 Issues

すでに明確に説明できる問題は、直接 Issue を開いてください。

[**GitHub Issues →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/issues)

例：

* 再現可能な不具合
* 明確に不足している機能
* データ解析エラー
* Technical Specifications 正規化エラー
* NFO 変更の問題
* UI 動作の問題
* リリース / インストールの問題
* 明確に定義された機能要望

診断を容易にするため、可能であれば該当バージョン、メディア種別、再現手順、エラー情報を含めてください。

---

## 🤝 コントリビューション

IMDb Tech Manager はオープンソースです。Fork、調査、変更、Pull Request を歓迎します。

コードを変更する前に、次の文書をお読みください。

[**`AGENTS.md` →**](../../AGENTS.md)

重要なアーキテクチャ規則、データ安全性の制約、テスト要件、リリース境界を説明しています。

特に次の領域を変更する前に、既存の設計を理解してください。

* NFO の読み書き
* Technical Specifications
* Technical Tags
* タグ所有権
* AI 呼び出し
* トークン / キャッシュ会計
* バッチタスク
* アプリケーションのライフサイクル
* プラットフォーム固有コード
* 更新とリリース

次の分野を含むコントリビューションを歓迎します。

* バグ修正
* 機能改善
* Technical Specifications 解析ルール
* 技術仕様の正規化
* カメラ / レンズ / 制作形式情報
* テストケース
* UI / UX 改善
* 性能と安定性の改善
* ドキュメント
* Coding Agent Context の改善

既存の動作を変更する場合は、1 つの問題の修正によって既存のメディアメタデータワークフローが壊れないよう、可能な限り適切なテストまたは回帰検証を追加してください。

---

## 📄 ライセンス

IMDb Tech Manager は **Apache License 2.0** の下で公開されているオープンソースソフトウェアです。

ライセンス全文：

[**LICENSE →**](../../LICENSE)

リポジトリには次の文書も含まれます。

[**NOTICE →**](../../NOTICE)

ソースコードを使用、変更、配布する場合は、Apache License 2.0 およびリポジトリに含まれる関連通知に従ってください。

---

## ⚠️ 免責事項

IMDb Tech Manager は独立して開発されたオープンソースプロジェクトです。

本プロジェクトは **IMDb、Emby、その他の第三者プラットフォームと公式に提携、承認、推奨されていません**。

第三者の名称、商標、データ、サービスは、それぞれの所有者に帰属します。

IMDb Tech Manager は、技術仕様の取得・処理およびメディアメタデータ管理のためのツールを提供します。

第三者のデータ、API、Web サイト、サービスの利用が、該当する利用規約、ライセンス要件、法律に従っていることを確認する責任はユーザーにあります。

---

## 💡 フィードバックと提案

IMDb Tech Manager は引き続き活発に開発されています。

次の内容に関するアイデアがあれば：

* IMDb Technical Specifications
* 技術仕様の正規化
* カメラとレンズ情報
* フィルムおよびデジタル撮影形式
* NFO メタデータ管理
* 技術タグルール
* AI 支援による意味処理
* UI / UX
* ITM と TCM の連携
* 他のオペレーティングシステムへの対応
* Coding Agent を利用する開発ワークフロー

Discussions または Issues へぜひご参加ください。

本プロジェクトは、実際の利用者からのフィードバックを基に、機能、データ規則、開発方針を継続して進化させます。
