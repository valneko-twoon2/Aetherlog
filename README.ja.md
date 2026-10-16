<div align="center">

# Aetherlog

**FINAL FANTASY XIV のログを読むための閲覧ツール（読み取り専用）**

[![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-backend-DEA584?logo=rust&logoColor=black)](https://www.rust-lang.org/)
[![Vue 3](https://img.shields.io/badge/Vue-3-42B883?logo=vuedotjs&logoColor=white)](https://vuejs.org/)

[English](README.md) · **日本語**

<img src="docs/images/main.png" alt="パーティチャットを表示した Aetherlog" width="860">

<sub>スクリーンショットは、架空のキャラクターと自動生成したデモログ（<a href="tools/make_demo_logs.ps1"><code>tools/make_demo_logs.ps1</code></a>）です。</sub>

</div>

---

## 特長

- **チャンネル別フィルター**: 会話・システム・イベント・戦闘のタブと、種別ごとのチップ。チップをダブルクリックすると、その種別だけを表示します。
- **読みやすい表示**: 種別ごとの色、パーティ番号、アイコン、リンク、色付き文字、定型文を再現します。
- **検索**: 発言者・本文で絞り込みます。
- **キャラクターと日付**: キャラクターを選び、1 日だけ、または全期間を表示します。
- **エクスポート**: 絞り込んだ結果を CSV または JSON に保存します。
- **ローカル保存**: ゲームはログを上書きするため、アプリ専用のコピーを保存します。ゲームが後から消したログも読めます。
- **読み取り専用**: ログを書き換えず、書き込むのはエクスポートの `.csv` / `.json` だけです。

## スクリーンショット

<table>
  <tr>
    <td align="center"><img src="docs/images/battle.png" alt="種別チップのある戦闘タブ" width="420"><br><sub>戦闘タブ（種別ごとのチップ）</sub></td>
    <td align="center"><img src="docs/images/menu.png" alt="キャラクターとログの場所のメニュー" width="420"><br><sub>メニュー（キャラクター・ログの場所）</sub></td>
  </tr>
</table>

## 導入

必要なもの: WebView2 が使える Windows（Windows 11 には標準で入っています）、[Rust](https://rustup.rs/)、[Node.js](https://nodejs.org/)。

```powershell
npm install --prefix web
powershell -File tools\publish.ps1
```

ビルドして、`artifacts` フォルダーに `Aetherlog.exe` を出力します（`tools\publish.bat` でも同じです。`tools\start.bat` で起動します）。`tools\preview.ps1` を使うと、Rust 側をビルドせずに、サンプルデータで画面をブラウザーで確かめられます。

コミットする場合は [`AGENTS.md`](AGENTS.md) を参照してください。

## 使い方

起動すると、前回開いた場所（初回は `ドキュメント\My Games\FINAL FANTASY XIV - A Realm Reborn`）にある `.log` を、すべて読み込みます。

- **メニュー**（`メニュー`）: キャラクターの選択、ログの場所の変更（フォルダー、ファイル、ドラッグ＆ドロップ）、再読み込み、アーカイブ済みログの削除ができます。ゲーム側の元のログには触れません。
- **エクスポート**（`エクスポート`）: 現在の絞り込み結果を、CSV（Excel 用の UTF-8、BOM 付き）または JSON に保存します。
- **プレイヤー名**: Ctrl を押しながらクリックすると、その名前でロードストーンをブラウザーで開きます。
- **キャラクター名**: ログには自分の名前が書かれていないため、パーティチャットから推測します。メニューで手入力もできます。
- **データ**: アーカイブと名前は、`Aetherlog.exe` と同じ場所の `userdata` に保存されます。

## デモログで試す

FFXIV がなくても、画面を確かめられます。架空のデモログを生成して、Aetherlog に読み込ませます。

```powershell
powershell -File tools\make_demo_logs.ps1
$env:AETHERLOG_LOGS = "docs\demo-logs"
$env:AETHERLOG_DATA = "$env:TEMP\aetherlog-demo"   # 実際のアーカイブや名前と混ざらないようにします
artifacts\Aetherlog.exe
```

## 免責事項

Aetherlog は非公式のファンメイドツールです。株式会社スクウェア・エニックスとは関係がなく、同社の承認も受けていません。FINAL FANTASY XIV、ロードストーン、およびゲームの内容や画像の権利は、株式会社スクウェア・エニックスに帰属します。

FINAL FANTASY XIV © SQUARE ENIX

## ライセンス

MIT ライセンスです。[LICENSE](LICENSE) を参照してください。対象は Aetherlog のソースコードだけで、表示するゲームの内容は含みません。
