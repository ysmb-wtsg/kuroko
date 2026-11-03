# ADR 0013: エージェントを Claude Code 専用にする

## ステータス

採用（2026-11-03）。ADR 0012 の決定6（Claude Code 以外のプロバイダーの扱い）を置き換える

## コンテキスト

エージェントは `AgentProvider` trait と `BuiltinProvider`（ClaudeCode / Codex / Custom）で
抽象化し、`krk.opt.main_pane = "codex"` で Codex も起動できた。しかしセッション一覧
（ADR 0012）のように、エージェント固有の仕組み（セッション履歴・再開・セッションID指定）に
踏み込む機能ほど抽象化のコストが大きくなる。実際の利用は Claude Code のみで、
Custom プロバイダーは設定から指定する手段も無く使われていなかった。

## 決定

1. **kuroko のエージェントは Claude Code 専用とする**。`AgentProvider` trait と
   `BuiltinProvider` を削除し、`claude_command(&SessionStart)` で起動コマンドを直接組み立てる
2. **`AgentPane` は常にセッションIDを持つ**（新規は `--session-id`、再開は `--resume`）
3. **`krk.opt.main_pane` は `"claude-code"` と `"terminal"` のみとする**。未知の値
   （廃止した `"codex"` を含む）は起動時に警告を出して `"claude-code"` で起動する

## 結果

- Claude Code 固有機能（セッション・フックなど）をそのまま作り込める
- `main_pane = "codex"` を設定していたユーザーは Claude Code で起動するようになる（警告で気づける）
- 他のエージェントを使いたい場合は、メインペインを `"terminal"` にして手動で起動する
