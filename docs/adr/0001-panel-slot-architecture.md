# ADR 0001: パネルスロットアーキテクチャ

- **Status**: Accepted
- **Date**: 2026-06-03

## Context

aijin のレイアウトには、メインペイン（中央）に加えて補助パネル（ファイルツリー、ターミナル、Gitパネル）が存在する。
これらの補助パネルをどのように管理するか、2つの選択肢があった。

1. **自由分割方式**: 全パネルを LayoutNode のバイナリ分割で均等に扱う
2. **固定スロット方式**: 補助パネルは固定の位置（左・下・右）にスロットとして配置し、トグルで表示/非表示を切り替える

## Decision

**固定スロット方式**を採用した。

App が `file_tree_id`, `git_panel_id` などのスロットID と `file_tree_visible`, `bottom_terminal_visible`, `git_panel_visible` の可視性フラグを保持する。
`rebuild_layout()` が可視フラグに基づいてレイアウトツリーを毎回再構築する。

## Consequences

- ユーザーは `f`, `g`, `t` などのキーで直感的にパネルをトグルできる
- パネルの位置が予測可能（ファイルツリーは常に左、Gitは常に右）
- レイアウトの自由度は制限される（補助パネルの位置は固定）
- パネル間のサイズ比率は `left_ratio`, `bottom_ratio`, `right_ratio` で管理
