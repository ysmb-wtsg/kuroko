# ADR 0004: Theme構造体への移行と役割ベースアクセントカラーへの削減

## ステータス

採用（2026-06-13）

## コンテキスト

デザイントークンは `aijin-core/src/theme.rs` の `pub const` 群で定義されていた。これには以下の問題があった。

1. **カスタマイズ不能**: Lua経由のテーマカスタマイズ（Planned機能）を実装する際、定数では実行時差し替えができない
2. **色数過多**: アクセント6色（BLUE/BLUE_LIGHT/AMBER/PURPLE/GREEN/YELLOW）+ フォーカスボーダー3色が存在し、オーバーレイごとに枠色が異なる（パレット=紫、プレビュー=青、プロンプト=黄、情報=水色）など、トンマナが分散していた
3. **トークン外のハードコード**: `Color::LightBlue` 等のANSI named colorが filetree とモード色に残っており、端末側パレット設定に依存して他のoklchトークンと調和しない

## 決定

1. **`Theme` 構造体 + `RwLock<Theme>` に移行する**。描画コードは `theme::get()` でトークンを参照し、`theme::set(Theme)` で実行時差し替えが可能。これがLuaテーマAPIのエントリポイントとなる
2. **アクセントカラーを役割ベースの4色に削減する**
   - `accent_primary`（青）: オーバーレイ枠、Normalモード、フォーカス表示、ディレクトリ名
   - `accent_positive`（緑）: Insertモード、SELECT選択マーカー
   - `accent_warning`（琥珀）: Selectモード、リネーム、削除確認、Working表示、コピーモード
   - `accent_error`（赤）: エラーメッセージ表示
3. **ANSI named colorを全廃する**。全トークンはoklchベースで設計した `Color::Rgb` 値とする
4. **フォーカスボーダー3色（ペイン種別ごと）は廃止**し、`border_focus` 1色に統一する（ADR 0003のセパレータ強調に使用）

## 結果

- `theme::get()` は `Theme` のコピーを返す（Copy、数百バイト）。フレーム毎の参照コストはRwLock readで実用上無視できる
- 既存の `SURFACE_PROMPT` は `surface_overlay` に統合（視覚差がほぼなかったため）
- モード色は専用トークンを持たず、アクセント4色から導出する（Normal=primary、Insert=positive、Select=warning）
- ロックがpoisonした場合は `into_inner` でフォールバックし、描画でpanicしない
