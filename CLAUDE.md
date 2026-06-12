# kuroko

AIエージェント時代の「エディタに代わるホームベース」を目指すターミナルTUIアプリケーション。
Neovimのようなカスタマイズ性・拡張性を持ちつつ、AIエージェント操作を中心に据える。

## 設計方針

- Neovimの設計を踏襲: 3層モデル（Buffer/Window/Tab）、名前空間API（`krk.*`）、イベント駆動
- プラグインは `.setup(opts)` 規約
- PTY読み取りは専用std::thread（メインスレッドをブロックしない）
- Lua→Rust通信はmpscチャネル経由のAction送信（再入問題回避）
- エージェントプロバイダーはtraitで抽象化（BuiltinProvider + カスタム）

## 注意点

- `vt100` はcrates.io版ではなく `vendor/vt100` のベンダリングフォーク。依存更新時にcrates.io版へ戻さないこと
- コピーモードは `Mode` enumのモードではなく、ターミナルペイン内部の状態。モード追加と混同しないこと

## Planned（未実装）

以下の機能は設計済みだが未実装。実装済みと誤認しないこと。

- **Lua pane.spawn(type)**: Luaからのペイン生成
- **Lua layout.split(direction)**: Luaからのペイン分割
- **セッション永続化（タブ）**: タブ構成の保存と復元（パネル表示状態・分割比は実装済み）
- **テーマカスタマイズ**: Lua経由のカラースキーム変更（`theme::set(Theme)`による実行時差し替え機構は実装済み、Lua APIが未実装）
