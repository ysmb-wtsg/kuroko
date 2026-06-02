# ADR 0002: PTY スレッドモデル

- **Status**: Accepted
- **Date**: 2026-06-03

## Context

ターミナルペインは PTY（疑似端末）からのデータを継続的に読み取り、vt100 パーサーに渡して画面状態を更新する必要がある。
PTY の read はブロッキング I/O であり、非同期ランタイムとの統合方法を決める必要があった。

## Decision

**専用の std::thread** で PTY 読み取りを行い、mpsc チャネル経由でメインループにデータを送信する。

PtyHandle がスレッドを起動し、`PtyMessage::Output(Vec<u8>)` でデータを送る。
メインループは `try_recv()` でノンブロッキングに読み取り、該当ペインの `process_output()` に渡す。

## Consequences

- メインの描画ループがブロックされない
- 非同期ランタイム（tokio等）への依存が不要
- スレッドごとのオーバーヘッドはあるが、ターミナルペイン数は通常少数なので問題にならない
- PTY の終了は `PtyMessage::Exit` で通知される
