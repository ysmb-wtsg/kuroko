# kuroko

AIエージェント時代の「エディタに代わるホームベース」を目指すターミナルTUIアプリケーション。コマンド名は `krk`。

Neovimのようなカスタマイズ性・拡張性を持ちつつ、AIエージェント操作を中心に据える。

名前は歌舞伎の「黒子」から。AIエージェントがコードと人間の間に立ち、ソースコードを黒衣のように覆い隠す——「黒い画面」の上で。

## 特徴

- **AIエージェント統合**: Claude Code, Codex, カスタムエージェントをPTYで埋め込み
- **パネル管理**: ファイルツリー・ターミナル・Gitパネルのトグル、リサイズ対応
- **タブシステム**: エージェントタブとターミナルタブを独立管理
- **モーダル操作**: Normal / Insert / Select の3モード（Vimライク）
- **Luaカスタマイズ**: `~/.config/krk/init.lua` で設定変更
- **Gitパネル**: lazygit / tig / gitui などの外部ツールを右パネルに埋め込み
- **ファイルツリー**: gitignore対応、ファイル操作（作成・リネーム・削除）、プレビュー

## インストール

### Homebrew

```sh
brew install ysmb-wtsg/tap/kuroko
```

### ソースからビルド

Rust 1.96.0 以上が必要（[mise](https://mise.jdx.dev/) で管理推奨）。

```sh
git clone https://github.com/ysmb-wtsg/kuroko.git
cd kuroko
cargo build --release
```

バイナリは `target/release/krk` に生成される。

## 使い方

```sh
krk
```

起動するとInsertモードでエージェントペインが表示される。

### モード切替

| キー | 動作 |
|------|------|
| `Esc` | Normalモードに切替 |
| `i` | Insertモードに戻る |

### Normalモードのキーバインド

| キー | 動作 |
|------|------|
| `h/j/k/l` | 方向フォーカス移動 |
| `Tab` / `Shift+Tab` | フォーカス順送り/逆送り |
| `H/J/K/L` | ペインリサイズ |
| `t` | ターミナルパネルのトグル |
| `f` | ファイルツリーパネルのトグル |
| `g` | Gitパネルのトグル |
| `q` | 終了 |

### タブ操作（フォーカス中のパネルに作用）

| キー | 動作 |
|------|------|
| `n` | 新しいタブを追加 |
| `x` / `w` | アクティブタブを閉じる |
| `[` / `]` | 前/次のタブに切り替え |
| `1-9` | 番号指定でタブ選択 |
| `r` | タブをリネーム |

## Luaカスタマイズ

`~/.config/krk/init.lua` に設定ファイルを配置すると起動時に読み込まれる。

```lua
-- 利用可能なAPI
krk.pane.toggle(type)      -- パネルのトグル（"terminal", "files"）
krk.pane.focus(direction)  -- フォーカス移動（"next", "prev", "left", "right", "up", "down"）
krk.opt.leader             -- リーダーキー設定
krk.opt.main_pane          -- メインペイン種別（"claude-code", "codex", "terminal"）
krk.opt.git_tool           -- Gitパネルのツール（"lazygit", "tig", "gitui"等）
```

## 技術スタック

- **言語**: Rust (edition 2024)
- **TUI**: ratatui 0.30 + crossterm
- **PTY**: portable-pty + vt100
- **プラグイン**: Lua 5.4 (mlua, vendored)

## ステータス

v0.1.0-alpha - 基本的なペイン管理・エージェント統合・Lua設定が動作する段階。

以下の機能は計画中：
- コマンドパレット
- カスタムキーバインド（`krk.keymap.set`）
- ターミナルコピーモード（スクロールバック）
- セッション永続化
- テーマカスタマイズ

## ライセンス

MIT
