<div align="center">

# Aethelog

**FINAL FANTASY XIV のログを読むための閲覧ツール（読み取り専用）**

[![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-backend-DEA584?logo=rust&logoColor=black)](https://www.rust-lang.org/)
[![Vue 3](https://img.shields.io/badge/Vue-3-42B883?logo=vuedotjs&logoColor=white)](https://vuejs.org/)

[English](README.md) · **日本語**

<img src="docs/images/main.png" alt="パーティチャットを表示した Aethelog" width="860">

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
- **自動更新**: 選択したログの場所を監視し、ゲームがログを書き込むたびに新しい行を保存済みのアーカイブに追記します。同じログの更新ごとにコピーのファイル数が増えることはありません。
- **アニメーション設定**: 背景の動き、読み込み中のアニメーション、画面効果をON/OFFできます。OSの視覚効果を減らす設定も尊重します。
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

ビルドして、`artifacts` フォルダーに `Aethelog.exe` と `Aethelog_v<version>.zip` を出力します（`tools\publish.bat` でも同じです。`tools\start.bat` で起動します）。`tools\preview.ps1` を使うと、Rust 側をビルドせずに、サンプルデータで画面をブラウザーで確かめられます。

コミットする場合は [`AGENTS.md`](AGENTS.md) を参照してください。

## 使い方

起動すると、前回開いた場所（初回は `ドキュメント\My Games\FINAL FANTASY XIV - A Realm Reborn`）にある `.log` を、すべて読み込みます。

自動保存・更新は起動時に有効になります。メニューの「ログを自動保存・更新する」で停止・再開できます。「表示設定」の「アニメーションを有効にする」でアニメーションをON/OFFできます。アニメーション設定は `userdata/preferences.json` に保存し、次回起動時に復元します。ゲームがログを書き込むと通知を受けて取り込みます。通知の取りこぼしに備えて、1分に1回も確認します。反映されるのはゲームがディスクに書き出した後の内容です。ゲームはログが1,000行たまったときか、ログアウト時にファイルを書き込むため、まだ保存されていないチャットは表示できません。新しいゲームログのセッションではアーカイブが増えますが、同じセッションへの追記ではスナップショットを増やさず、既存のファイルに追加します。自動更新中も絞り込みと閲覧位置を維持し、末尾を表示中は新しい行に追従します。キャラクターのアーカイブを削除すると自動更新を停止し、元のログがすぐ取り込まれ直すことを防ぎます。自動更新を再開するか、再読み込みすると取り込み直されます。

- **メニュー**（`メニュー`）: キャラクターの選択、ログの場所の変更（フォルダー、ファイル、ドラッグ＆ドロップ）、再読み込み、アーカイブ済みログの削除ができます。ゲーム側の元のログには触れません。
- **エクスポート**（`エクスポート`）: 現在の絞り込み結果を、CSV（Excel 用の UTF-8、BOM 付き）または JSON に保存します。
- **プレイヤー名**: Ctrl を押しながらクリックすると、その名前でロードストーンをブラウザーで開きます。
- **キャラクター名**: ログには自分の名前が書かれていないため、パーティチャットから推測します。メニューで手入力もできます。
- **データ**: アーカイブ・名前・設定は、`Aethelog.exe` と同じ場所の `userdata` に保存されます。

## デモログで試す

FFXIV がなくても、画面を確かめられます。架空のデモログを生成して、Aethelog に読み込ませます。

```powershell
powershell -File tools\make_demo_logs.ps1
$env:AETHELOG_LOGS = "docs\demo-logs"
$env:AETHELOG_DATA = "$env:TEMP\aethelog-demo"   # 実際のアーカイブや名前と混ざらないようにします
artifacts\Aethelog.exe
```

## 免責事項

Aethelog は非公式のファンメイドツールです。株式会社スクウェア・エニックスとは関係がなく、同社の承認も受けていません。FINAL FANTASY XIV、ロードストーン、およびゲームの内容や画像の権利は、株式会社スクウェア・エニックスに帰属します。

FINAL FANTASY XIV © SQUARE ENIX

## ライセンス

MIT ライセンスです。[LICENSE](LICENSE) を参照してください。対象は Aethelog のソースコードだけで、表示するゲームの内容は含みません。サードパーティのライセンスは、[`THIRD-PARTY-NOTICES.txt`](THIRD-PARTY-NOTICES.txt) とアプリ内（`Aethelog について`）に載せています。
