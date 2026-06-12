# ADR 0006: kurokoへの改名とHomebrew配布方式

## ステータス

採用（2026-06-13）

## コンテキスト

公開準備にあたり、プロジェクト名「aijin」を再検討した。

1. **名前の問題**: 「aijin」は「AI人」の意図だが、日本語ローマ字読みでは「愛人」（不倫相手の含意）が第一想起となり、公開プロダクト名として誤解リスクが大きい
2. **プロジェクト思想**: 本プロジェクトは「エディタ＝ソースコードと人間のインターフェース」という旧パラダイムから、「AIエージェントと人間のインターフェース」という新パラダイムへの転換を体現する。AIエージェントがコードの前に立ち、ソースコードを人間にとってブラックボックス化する
3. **配布**: Homebrewでインストール可能にする必要がある

## 決定

### 名前: kuroko（コマンド名 krk）

- 歌舞伎の「黒子」: 舞台上に存在するが「見えないことにする」介在者。AIエージェントがコードと人間の間に立つ構図のメタファー。ターミナル＝「黒い画面」にもちなむ
- **ripgrep→rg方式を採用**: 正式名 `kuroko`（検索性・発音を担保）、コマンド名 `krk`（打鍵性を担保）
- 素のkurokoはGitHub上に kuroko-lang（Python方言、482★）、cookpad/kuroko2（ジョブスケジューラ、320★）が存在するが、ドメインが異なり、crates.io・Homebrew formula名は空きのため許容
- `krk` 単体は crates.io / brew / GitHub いずれも競合なし

### 命名の波及範囲

- クレート: `aijin-*` → `kuroko-*`
- バイナリ: `krk`（`[[bin]]` で指定）
- Lua名前空間: `aijin.*` → `krk.*`（nvimが`vim.*`を使うのと同様、ユーザーが日常的に打つ名前はコマンド名に揃える）
- configディレクトリ: `~/.config/aijin/` → `~/.config/krk/`（nvimの慣習に倣いコマンド名と一致）

### 配布: tapリポジトリ + ソースビルドformula

- `ysmb-wtsg/homebrew-tap` にformulaを置き、`brew install ysmb-wtsg/tap/kuroko` でインストール
- formulaはGitHubのタグtarballから `cargo install` でビルドする（CI不要の最小構成）
- バイナリ配布（cargo-dist）はインストール時間が問題になった時点で再検討する

## 影響

- 既存ADR（0001〜0005）内の「aijin」表記はappend-only原則により修正しない。本ADRをもって読み替える
- `--version` / `-V` フラグをバイナリに追加（Homebrewのformulaテストで使用）
- 既存ユーザーの `~/.config/aijin/` は自動移行しない（公開前のため既存ユーザーは実質開発者のみ）
