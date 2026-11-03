//! PTYベースのターミナルペインの実装。
//! Paneトレイトを実装し、シェルの起動・入力・描画を管理する。

use std::any::Any;
use std::sync::mpsc;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;

use kuroko_core::theme;
use kuroko_core::{Action, AppEvent, Pane, PaneId, PaneType};

use crate::pty_handle::{PtyHandle, PtyMessage};
use crate::widget::TerminalWidget;

/// コピーモードの選択種別。Vimの `v`（文字単位）と `V`（行単位）に対応する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    /// 文字単位の範囲選択
    Char,
    /// 行単位の範囲選択
    Line,
}

/// 選択開始位置。スクロールに依存しない論理位置に変換できるよう、
/// アンカーを張った時点のスクロールオフセットと画面上の行・列を保持する。
#[derive(Debug, Clone, Copy)]
struct SelectionAnchor {
    scroll_offset: usize,
    row: u16,
    col: u16,
    kind: SelectionKind,
}

/// 論理座標で正規化した選択範囲（start <= end）。
/// 論理行は「画面行 - スクロールオフセット」で、値が大きいほど新しい行。
#[derive(Debug, Clone, Copy)]
struct SelectionRange {
    start: (isize, u16),
    end: (isize, u16),
    kind: SelectionKind,
}

impl SelectionRange {
    /// 論理行 `row`・列 `col` のセルが選択範囲内かを判定する
    fn contains(&self, row: isize, col: u16) -> bool {
        if row < self.start.0 || row > self.end.0 {
            return false;
        }
        match self.kind {
            SelectionKind::Line => true,
            SelectionKind::Char => {
                (row > self.start.0 || col >= self.start.1)
                    && (row < self.end.0 || col <= self.end.1)
            }
        }
    }
}

/// 単語移動（w/b/e）用の文字クラス。Vimの `word` 境界判定に準じる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Blank,
    Word,
    Punct,
}

/// PTYベースのターミナルペイン。
/// シェルプロセスを内包し、vt100パーサーで画面状態を管理する。
pub struct TerminalPane {
    /// ペインの一意ID
    id: PaneId,
    /// 表示タイトル
    title: String,
    /// PTYプロセスのハンドル
    pty: Option<PtyHandle>,
    /// vt100ターミナルパーサー
    parser: vt100::Parser,
    /// 直前の描画サイズ（リサイズ検出用）
    last_size: (u16, u16),
    /// PTYスポーン失敗時のエラーメッセージ
    spawn_error: Option<String>,
    /// PTY書き込み失敗によりPTYが使用不能になったことを示すフラグ
    pty_dead: bool,
    /// コピーモード有効フラグ
    copy_mode: bool,
    /// コピーモード時のスクロールオフセット（0 = 最新画面）
    scroll_offset: usize,
    /// コピーモード時のカーソル位置（画面上の行, 列）
    copy_cursor: (u16, u16),
    /// テキスト選択の開始位置（未選択ならNone）
    selection_anchor: Option<SelectionAnchor>,
}

impl TerminalPane {
    /// 新しいTerminalPaneを生成し、PTYプロセスを起動する。
    ///
    /// @param id - ペインID
    /// @param cols - 初期列数
    /// @param rows - 初期行数
    /// @param pty_sender - PTY出力の送信先チャネル
    /// @returns TerminalPaneインスタンス
    pub fn new(id: PaneId, cols: u16, rows: u16, pty_sender: mpsc::Sender<PtyMessage>) -> Self {
        let (pty, spawn_error) = match PtyHandle::spawn(id, cols, rows, pty_sender) {
            Ok(handle) => (Some(handle), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let parser = vt100::Parser::new(rows, cols, 10000);

        Self {
            id,
            title: "Terminal".to_string(),
            pty,
            parser,
            last_size: (cols, rows),
            spawn_error,
            pty_dead: false,
            copy_mode: false,
            scroll_offset: 0,
            copy_cursor: (0, 0),
            selection_anchor: None,
        }
    }

    /// 指定コマンドを実行するTerminalPaneを生成する。
    /// lazygit等の外部TUIツールの埋め込みに使用する。
    ///
    /// @param id - ペインID
    /// @param program - 実行するプログラム名
    /// @param args - プログラムの引数
    /// @param title - ペインの表示タイトル
    /// @param cols - 初期列数
    /// @param rows - 初期行数
    /// @param pty_sender - PTY出力の送信先チャネル
    /// @returns TerminalPaneインスタンス
    pub fn with_command(
        id: PaneId,
        program: &str,
        args: &[&str],
        title: &str,
        cols: u16,
        rows: u16,
        pty_sender: mpsc::Sender<PtyMessage>,
    ) -> Self {
        use portable_pty::CommandBuilder;

        let mut cmd = CommandBuilder::new(program);
        for arg in args {
            cmd.arg(arg);
        }
        cmd.cwd(std::env::current_dir().unwrap_or_else(|_| "/".into()));

        Self::from_command(id, title, cols, rows, pty_sender, cmd)
    }

    /// CommandBuilderからTerminalPaneを生成する。
    /// AgentPaneなど外部から任意のコマンドで起動する場合に使用する。
    ///
    /// @param id - ペインID
    /// @param title - ペインの表示タイトル
    /// @param cols - 初期列数
    /// @param rows - 初期行数
    /// @param pty_sender - PTY出力の送信先チャネル
    /// @param cmd - 起動するコマンド
    /// @returns TerminalPaneインスタンス
    pub fn from_command(
        id: PaneId,
        title: &str,
        cols: u16,
        rows: u16,
        pty_sender: mpsc::Sender<PtyMessage>,
        cmd: portable_pty::CommandBuilder,
    ) -> Self {
        let (pty, spawn_error) =
            match PtyHandle::spawn_with_command(id, cols, rows, pty_sender, cmd) {
                Ok(handle) => (Some(handle), None),
                Err(e) => (None, Some(e.to_string())),
            };
        let parser = vt100::Parser::new(rows, cols, 10000);

        Self {
            id,
            title: title.to_string(),
            pty,
            parser,
            last_size: (cols, rows),
            spawn_error,
            pty_dead: false,
            copy_mode: false,
            scroll_offset: 0,
            copy_cursor: (0, 0),
            selection_anchor: None,
        }
    }

    /// コピーモードに入る。
    /// スクロールオフセットを0（最新画面）にリセットする。
    pub fn enter_copy_mode(&mut self) {
        self.copy_mode = true;
        self.scroll_offset = 0;
        self.copy_cursor = (0, 0);
        self.selection_anchor = None;
    }

    /// コピーモードを終了する。
    /// スクロールオフセット、カーソル、選択をリセットし、パーサーのスクロールバックも解除する。
    pub fn exit_copy_mode(&mut self) {
        self.copy_mode = false;
        self.scroll_offset = 0;
        self.copy_cursor = (0, 0);
        self.selection_anchor = None;
        self.parser.set_scrollback(0);
    }

    /// コピーモード中かどうかを返す。
    ///
    /// @returns コピーモードが有効ならtrue
    pub fn is_copy_mode(&self) -> bool {
        self.copy_mode
    }

    /// 現在のスクロールオフセットを返す。
    /// 0が最新画面、値が大きいほど過去の行を表示中。
    ///
    /// @returns スクロールオフセット（行数）
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// スクロールバッファを上方向（過去）にスクロールする。
    /// スクロールバッファの最大行数を超えないようクランプされる。
    ///
    /// @param lines - スクロールする行数
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
        self.parser.set_scrollback(self.scroll_offset);
        // set_scrollback内部でバッファ長にクランプされるので、実際の値を同期する
        self.scroll_offset = self.parser.screen().scrollback();
    }

    /// スクロールバッファを下方向（最新）にスクロールする。
    /// オフセット0（最新画面）を下回らない。
    ///
    /// @param lines - スクロールする行数
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.parser.set_scrollback(self.scroll_offset);
    }

    /// 最大スクロールオフセット（スクロールバッファの先頭）まで移動する。
    pub fn scroll_to_top(&mut self) {
        self.parser.set_scrollback(usize::MAX);
        // set_scrollback内部でバッファ長にクランプされるので、実際の値を同期する
        self.scroll_offset = self.parser.screen().scrollback();
    }

    /// スクロールオフセットを0（最新画面）にリセットする。
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
        self.parser.set_scrollback(0);
    }

    /// コピーモードのカーソル位置を返す（行, 列）
    pub fn copy_cursor(&self) -> (u16, u16) {
        self.copy_cursor
    }

    /// コピーモードのカーソルを上に移動する。
    /// 画面上端に達したらスクロールアップする。
    pub fn move_cursor_up(&mut self) {
        if self.copy_cursor.0 > 0 {
            self.copy_cursor.0 -= 1;
        } else {
            self.scroll_up(1);
        }
        self.snap_cursor_to_char_head();
    }

    /// コピーモードのカーソルを下に移動する。
    /// 画面下端に達したらスクロールダウンする。
    pub fn move_cursor_down(&mut self) {
        let max_row = self.last_size.1.saturating_sub(2); // インジケータ行を除く
        if self.copy_cursor.0 < max_row {
            self.copy_cursor.0 += 1;
        } else {
            self.scroll_down(1);
        }
        self.snap_cursor_to_char_head();
    }

    /// コピーモードのカーソルを左に移動する。全角文字は1文字（2セル）として飛ばす
    pub fn move_cursor_left(&mut self) {
        self.copy_cursor.1 = self.copy_cursor.1.saturating_sub(1);
        self.snap_cursor_to_char_head();
    }

    /// コピーモードのカーソルを右に移動する
    /// 全角文字は1文字（2セル）として飛ばす。
    pub fn move_cursor_right(&mut self) {
        let max_col = self.last_size.0.saturating_sub(1);
        let (row, col) = self.copy_cursor;
        let width = match self.parser.screen().cell(row, col) {
            Some(cell) if cell.is_wide() => 2,
            _ => 1,
        };
        // 行末の全角文字の上では、それ以上右へ進まない
        if col + width <= max_col {
            self.copy_cursor.1 = col + width;
        }
    }

    /// カーソルが全角文字の後半セルにあれば前半セルへ寄せる。
    /// 後半セル上ではカーソル表示が文字に重ならず、移動が止まったように見えるため。
    fn snap_cursor_to_char_head(&mut self) {
        let (row, col) = self.copy_cursor;
        let on_continuation = self
            .parser
            .screen()
            .cell(row, col)
            .is_some_and(|cell| cell.is_wide_continuation());
        if on_continuation && col > 0 {
            self.copy_cursor.1 = col - 1;
        }
    }

    /// カーソルを行頭（0列目）へ移動する（Vimの `0`）
    pub fn move_cursor_line_start(&mut self) {
        self.copy_cursor.1 = 0;
    }

    /// カーソルを行の最初の非空白文字へ移動する（Vimの `^`）
    pub fn move_cursor_first_non_blank(&mut self) {
        let row = self.copy_cursor.0;
        self.copy_cursor.1 = (0..self.screen_cols())
            .find(|&col| self.char_class(row, col) != CharClass::Blank)
            .unwrap_or(0);
    }

    /// カーソルを行の最後の非空白文字へ移動する（Vimの `$`）
    pub fn move_cursor_line_end(&mut self) {
        let row = self.copy_cursor.0;
        self.copy_cursor.1 = (0..self.screen_cols())
            .rev()
            .find(|&col| self.char_class(row, col) != CharClass::Blank)
            .unwrap_or(0);
    }

    /// 次の単語の先頭へ移動する（Vimの `w`）。行末を越えると次の行へ進む。
    pub fn move_word_forward(&mut self) {
        let start_class = self.cursor_class();
        // 現在の単語を抜ける。改行は単語境界として扱う
        if start_class != CharClass::Blank {
            loop {
                match self.step_forward() {
                    None => return,
                    Some(true) => break,
                    Some(false) if self.cursor_class() != start_class => break,
                    Some(false) => {}
                }
            }
        }
        // 空白を読み飛ばす
        while self.cursor_class() == CharClass::Blank {
            if self.step_forward().is_none() {
                return;
            }
        }
    }

    /// 前の単語の先頭へ移動する（Vimの `b`）。行頭を越えると前の行へ戻る。
    pub fn move_word_backward(&mut self) {
        if self.step_backward().is_none() {
            return;
        }
        while self.cursor_class() == CharClass::Blank {
            if self.step_backward().is_none() {
                return;
            }
        }
        self.extend_within_word(false);
    }

    /// 単語の末尾へ移動する（Vimの `e`）。既に末尾なら次の単語の末尾へ進む。
    pub fn move_word_end(&mut self) {
        if self.step_forward().is_none() {
            return;
        }
        while self.cursor_class() == CharClass::Blank {
            if self.step_forward().is_none() {
                return;
            }
        }
        self.extend_within_word(true);
    }

    /// 同じ文字クラスが続く限り、行内でカーソルを前方（forward=true）または後方へ進める
    fn extend_within_word(&mut self, forward: bool) {
        let (row, mut col) = self.copy_cursor;
        let class = self.char_class(row, col);
        let cols = self.screen_cols();
        if forward {
            while col + 1 < cols && self.char_class(row, col + 1) == class {
                col += 1;
            }
        } else {
            while col > 0 && self.char_class(row, col - 1) == class {
                col -= 1;
            }
        }
        self.copy_cursor.1 = col;
        self.snap_cursor_to_char_head();
    }

    /// カーソルを1文字前方へ進める。行末なら次の行の先頭へ移る（必要に応じてスクロール）。
    ///
    /// @returns 移動できなければNone、行をまたいだらSome(true)、同じ行内ならSome(false)
    fn step_forward(&mut self) -> Option<bool> {
        if self.copy_cursor.1 + 1 < self.screen_cols() {
            self.copy_cursor.1 += 1;
            return Some(false);
        }
        let before = (self.copy_cursor.0, self.scroll_offset);
        self.move_cursor_down();
        if (self.copy_cursor.0, self.scroll_offset) == before {
            return None;
        }
        self.copy_cursor.1 = 0;
        Some(true)
    }

    /// カーソルを1文字後方へ戻す。行頭なら前の行の末尾へ移る（必要に応じてスクロール）。
    ///
    /// @returns 移動できなければNone、行をまたいだらSome(true)、同じ行内ならSome(false)
    fn step_backward(&mut self) -> Option<bool> {
        if self.copy_cursor.1 > 0 {
            self.copy_cursor.1 -= 1;
            return Some(false);
        }
        let before = (self.copy_cursor.0, self.scroll_offset);
        self.move_cursor_up();
        if (self.copy_cursor.0, self.scroll_offset) == before {
            return None;
        }
        self.copy_cursor.1 = self.screen_cols().saturating_sub(1);
        Some(true)
    }

    /// 画面の列数
    fn screen_cols(&self) -> u16 {
        self.parser.screen().size().1
    }

    /// カーソル位置の文字クラス
    fn cursor_class(&self) -> CharClass {
        self.char_class(self.copy_cursor.0, self.copy_cursor.1)
    }

    /// 画面上の指定セルの文字クラスを返す。
    /// 全角文字の後半セルは前半セルと同じクラスとして扱う。
    fn char_class(&self, row: u16, col: u16) -> CharClass {
        let screen = self.parser.screen();
        let Some(cell) = screen.cell(row, col) else {
            return CharClass::Blank;
        };
        if cell.is_wide_continuation() && col > 0 {
            return self.char_class(row, col - 1);
        }
        match cell.contents().chars().next() {
            None => CharClass::Blank,
            Some(c) if c.is_whitespace() => CharClass::Blank,
            // 日本語などの非ASCII文字も単語構成文字として扱う
            Some(c) if c.is_alphanumeric() || c == '_' || !c.is_ascii() => CharClass::Word,
            Some(_) => CharClass::Punct,
        }
    }

    /// テキスト選択を開始/切替/解除する（Vimの v / V と同じ挙動）。
    /// 未選択なら現在のカーソル位置をアンカーに選択を開始する。
    /// 同じ種別で選択中なら解除し、別種別で選択中ならアンカーを保ったまま種別を切り替える。
    ///
    /// @param kind - 選択種別（文字単位/行単位）
    pub fn toggle_selection(&mut self, kind: SelectionKind) {
        match self.selection_anchor.as_mut() {
            Some(anchor) if anchor.kind == kind => self.selection_anchor = None,
            Some(anchor) => anchor.kind = kind,
            None => {
                // スクロールオフセットと画面位置を保存して絶対位置を記録する
                self.selection_anchor = Some(SelectionAnchor {
                    scroll_offset: self.scroll_offset,
                    row: self.copy_cursor.0,
                    col: self.copy_cursor.1,
                    kind,
                });
            }
        }
    }

    /// 現在のアンカーとカーソルから、正規化済みの選択範囲を計算する
    fn selection_range(&self) -> Option<SelectionRange> {
        let anchor = self.selection_anchor?;
        let a = (
            Self::logical_row(anchor.scroll_offset, anchor.row),
            anchor.col,
        );
        let c = (
            Self::logical_row(self.scroll_offset, self.copy_cursor.0),
            self.copy_cursor.1,
        );
        let (start, end) = if a <= c { (a, c) } else { (c, a) };
        Some(SelectionRange {
            start,
            end,
            kind: anchor.kind,
        })
    }

    /// 選択中かどうかを返す
    pub fn has_selection(&self) -> bool {
        self.selection_anchor.is_some()
    }

    /// 選択範囲のテキストを取得する。
    /// 選択なしならカーソル行のテキストを返す。
    ///
    /// @returns 選択範囲またはカーソル行のテキスト
    pub fn selected_text(&mut self) -> String {
        let Some(range) = self.selection_range() else {
            // 選択なし: カーソル行1行のみをコピーする
            let row = self.copy_cursor.0;
            let screen = self.parser.screen();
            let cols = screen.size().1;
            return screen.contents_between(row, 0, row, cols);
        };
        let (start_logical, start_col) = range.start;
        let (end_logical, end_col) = range.end;

        // 選択範囲がスクロールバック領域へ伸びる場合に備えてscroll_offsetを調整する。
        // 論理行Lが画面行Rになるには R = L + offset。
        // start_logicalが負（過去）なら画面行0に配置するため offset = -start_logical。
        // start_logicalが0以上（現在画面内）なら調整不要で offset = 0。
        let needed_offset = (-start_logical).max(0) as usize;
        let original_offset = self.scroll_offset;
        self.parser.set_scrollback(needed_offset);

        // 調整後の画面行に変換してから範囲を読み出す。
        // 以前は常に画面行0始点で読み出しており、現在画面内の選択で
        // 誤った先頭行（例: 6-10行選択で1-5行）をコピーするバグがあった。
        let start_row = (start_logical + needed_offset as isize) as u16;
        let end_row = (end_logical + needed_offset as isize) as u16;
        let screen = self.parser.screen();
        let cols = screen.size().1;
        // 行単位なら各行全体、文字単位ならアンカー〜カーソルの列まで（終端列を含む）
        let (from_col, to_col) = match range.kind {
            SelectionKind::Line => (0, cols),
            SelectionKind::Char => (start_col, end_col.saturating_add(1).min(cols)),
        };
        let text = screen.contents_between(start_row, from_col, end_row, to_col);

        // 元のスクロール位置に戻す
        self.parser.set_scrollback(original_offset);
        text
    }

    /// 画面上の行とスクロールオフセットから論理行番号を計算する。
    /// 論理行番号が大きいほど新しい（画面下方向）。
    /// scroll_offset が大きいほど古い（画面上方向）行を表示しているので、引く。
    fn logical_row(scroll_offset: usize, screen_row: u16) -> isize {
        (screen_row as isize) - (scroll_offset as isize)
    }

    /// 指定セル（画面上の行・列）が選択範囲内かどうかを判定する。
    pub fn is_cell_selected(&self, row: u16, col: u16) -> bool {
        self.selection_range()
            .is_some_and(|range| range.contains(Self::logical_row(self.scroll_offset, row), col))
    }

    /// 現在の画面内容をテキストとして返す（クリップボードコピー用）。
    ///
    /// @returns 画面内容のプレーンテキスト
    pub fn screen_text(&self) -> String {
        self.parser.screen().contents()
    }

    /// ターミナル内容を指定領域に描画する（ボーダーなし）。
    /// リサイズ検出とPTY/パーサーの更新も行う。
    /// AgentPaneなど、独自のボーダー描画を行うペインから利用する。
    ///
    /// @param frame - ratatuiの描画フレーム
    /// @param area - ターミナル内容を描画する領域
    /// フォーカス時に表示すべき端末カーソルの絶対座標を計算する。
    /// コピーモード中・スクロール中・カーソル非表示（DECTCEM）時はNoneを返す。
    ///
    /// @param area - 本体の描画領域（render_contentに渡すのと同じ領域）
    /// @returns カーソルの絶対座標 (x, y)。表示不要ならNone
    pub fn compute_cursor_position(&self, area: Rect) -> Option<(u16, u16)> {
        // コピーモード中はブロックカーソルをウィジェット側で反転表示するため、
        // また過去ログをスクロール表示中はカーソル位置が無意味なため表示しない
        if self.copy_mode || self.scroll_offset > 0 {
            return None;
        }
        let screen = self.parser.screen();
        if screen.hide_cursor() {
            return None;
        }
        let (row, col) = screen.cursor_position();
        let x = area.x + col;
        let y = area.y + row;
        // 描画領域内に収まる場合のみ表示する
        if x < area.right() && y < area.bottom() {
            Some((x, y))
        } else {
            None
        }
    }

    pub fn render_content(&mut self, frame: &mut Frame, area: Rect) {
        // PTYスポーン失敗時はエラーメッセージを表示
        if let Some(ref err) = self.spawn_error {
            let msg = format!("Failed to start process: {err}");
            let text =
                ratatui::text::Text::styled(msg, Style::default().fg(theme::get().accent_error));
            frame.render_widget(text, area);
            return;
        }

        let new_size = (area.width, area.height);
        if new_size != self.last_size && area.width > 0 && area.height > 0 {
            self.last_size = new_size;
            self.parser.set_size(area.height, area.width);
            if let Some(ref pty) = self.pty {
                let _ = pty.resize(area.width, area.height);
            }
        }

        // コピーモード時はスクロールオフセットをパーサーに反映する
        if self.copy_mode {
            self.parser.set_scrollback(self.scroll_offset);
        }

        let mut widget = TerminalWidget::new(self.parser.screen());
        if self.copy_mode {
            let range = self.selection_range();
            let scroll_offset = self.scroll_offset;
            widget = widget.with_copy_mode(self.copy_cursor, move |row, col| {
                range.is_some_and(|r| r.contains(Self::logical_row(scroll_offset, row), col))
            });
        }
        frame.render_widget(widget, area);

        // コピーモードのインジケータはステータスバー側（render_status_bar）でタイトル横に表示する

        // PTY終了時は最下行に終了インジケータを表示（コピーモード中は上書きしない）
        if self.pty_dead && !self.copy_mode && area.height > 0 {
            let indicator = ratatui::text::Span::styled(
                " [Process exited] ",
                Style::default().fg(theme::get().text_muted),
            );
            let indicator_area = Rect {
                x: area.x,
                y: area.y + area.height.saturating_sub(1),
                width: area.width,
                height: 1,
            };
            frame.render_widget(indicator, indicator_area);
        }
    }

    /// PTYから受信したデータをvt100パーサーに流し込む。
    /// パース後、端末問い合わせ（DA1等）への応答が生成されていれば
    /// PTYへ書き戻す。これをしないとyazi等のTUIが応答待ちでタイムアウトする。
    ///
    /// @param data - PTYからの出力バイト列
    pub fn process_output(&mut self, data: &[u8]) {
        self.parser.process(data);
        let response = self.parser.take_response();
        if !response.is_empty() {
            self.write_to_pty(&response);
        }
    }

    /// PTYにバイト列を書き込む（キー入力の転送）。
    /// 書き込みに失敗した場合、PTYが使用不能になったことを記録する。
    ///
    /// @param data - 書き込むバイト列
    pub fn write_to_pty(&mut self, data: &[u8]) {
        if let Some(ref mut pty) = self.pty
            && pty.write(data).is_err()
        {
            self.pty_dead = true;
        }
    }

    /// PTYスポーン失敗時のエラーメッセージを返す。
    /// App側で通知表示に使用する。
    pub fn spawn_error(&self) -> Option<&str> {
        self.spawn_error.as_deref()
    }

    /// PTYプロセスが終了済みかどうかを設定する。
    /// `PtyMessage::Exited` 受信時にApp側から呼び出される。
    pub fn set_pty_dead(&mut self) {
        self.pty_dead = true;
    }

    /// PTYが使用不能かどうかを返す
    pub fn is_pty_dead(&self) -> bool {
        self.pty_dead
    }
}

impl Pane for TerminalPane {
    fn id(&self) -> PaneId {
        self.id
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _focused: bool) {
        // ボーダーは描画しない。ペイン境界はApp側のセパレータ線が担い、
        // フォーカス表示はセパレータ色とステータスバーで行う。
        self.render_content(frame, area);
    }

    fn handle_event(&mut self, _event: &AppEvent) -> Vec<Action> {
        // イベント処理はApp側でモードに応じてルーティングされるため、
        // ペイン自身は特別なイベント処理を行わない
        Vec::new()
    }

    fn wants_raw_input(&self) -> bool {
        true
    }

    fn pane_type(&self) -> PaneType {
        PaneType::Terminal
    }

    fn cursor_position(&self, area: Rect) -> Option<(u16, u16)> {
        self.compute_cursor_position(area)
    }

    fn process_output(&mut self, data: &[u8]) {
        TerminalPane::process_output(self, data);
    }

    fn write_to_pty(&mut self, data: &[u8]) {
        TerminalPane::write_to_pty(self, data);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuroko_core::PaneId;

    /// テスト用のTerminalPaneを生成する（PTYなし）。
    /// 実際のシェルプロセスを起動せず、パーサーのみで動作確認する。
    fn create_test_pane() -> TerminalPane {
        TerminalPane {
            id: PaneId(999),
            title: "Test".to_string(),
            pty: None,
            parser: vt100::Parser::new(24, 80, 10000),
            last_size: (80, 24),
            spawn_error: None,
            pty_dead: false,
            copy_mode: false,
            scroll_offset: 0,
            copy_cursor: (0, 0),
            selection_anchor: None,
        }
    }

    #[test]
    fn process_output_updates_screen() {
        let mut pane = create_test_pane();
        pane.process_output(b"hello world");
        let screen = pane.parser.screen();
        // vt100パーサーに書き込んだテキストが画面内容に反映されることを確認
        let row = screen.contents_between(0, 0, 0, 80);
        assert!(row.contains("hello world"));
    }

    #[test]
    fn write_to_pty_without_pty_is_noop() {
        let mut pane = create_test_pane();
        // PTYがない状態でwrite_to_ptyを呼んでもパニックしないことを確認
        pane.write_to_pty(b"test");
    }

    #[test]
    fn da1_query_produces_vt102_response() {
        // DA1要求（CSI c）にVT102互換応答を返す。これがないとyazi等が
        // read_until_da1でタイムアウトし起動が遅延する。
        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(b"\x1b[c");
        assert_eq!(parser.take_response(), b"\x1b[?6c");
    }

    #[test]
    fn da1_with_explicit_zero_param_responds() {
        // CSI 0 c も同じくPrimary DA要求として応答する
        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(b"\x1b[0c");
        assert_eq!(parser.take_response(), b"\x1b[?6c");
    }

    #[test]
    fn secondary_da_is_ignored() {
        // Secondary DA（CSI > c）には応答しない
        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(b"\x1b[>c");
        assert!(parser.take_response().is_empty());
    }

    #[test]
    fn plain_output_produces_no_response() {
        // 通常のテキスト出力では応答を生成しない
        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(b"hello world");
        assert!(parser.take_response().is_empty());
    }

    #[test]
    fn take_response_clears_buffer() {
        // take_responseは取り出し後にバッファをクリアする
        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(b"\x1b[c");
        assert!(!parser.take_response().is_empty());
        assert!(parser.take_response().is_empty());
    }

    #[test]
    fn process_output_with_da1_does_not_panic_without_pty() {
        // PTYなしでもDA1応答の書き戻し経路がパニックしないことを確認
        let mut pane = create_test_pane();
        pane.process_output(b"\x1b[c");
    }

    #[test]
    fn pane_type_is_terminal() {
        let pane = create_test_pane();
        assert_eq!(pane.pane_type(), PaneType::Terminal);
    }

    #[test]
    fn pane_title() {
        let pane = create_test_pane();
        assert_eq!(pane.title(), "Test");
    }

    #[test]
    fn pane_id() {
        let pane = create_test_pane();
        assert_eq!(pane.id(), PaneId(999));
    }

    #[test]
    fn wants_raw_input() {
        let pane = create_test_pane();
        assert!(pane.wants_raw_input());
    }

    #[test]
    fn handle_event_returns_empty() {
        let mut pane = create_test_pane();
        let event = AppEvent::Tick;
        // TerminalPaneはイベントをApp側に委譲するため、空のVecを返す
        let actions = pane.handle_event(&event);
        assert!(actions.is_empty());
    }

    #[test]
    fn copy_mode_enter_exit() {
        let mut pane = create_test_pane();
        assert!(!pane.is_copy_mode());

        pane.enter_copy_mode();
        assert!(pane.is_copy_mode());
        assert_eq!(pane.scroll_offset, 0);

        pane.exit_copy_mode();
        assert!(!pane.is_copy_mode());
        assert_eq!(pane.scroll_offset, 0);
    }

    #[test]
    fn copy_mode_scroll_without_scrollback() {
        let mut pane = create_test_pane();
        pane.enter_copy_mode();

        // スクロールバッファが空の場合、scroll_upしてもオフセットは0のまま
        pane.scroll_up(5);
        assert_eq!(pane.scroll_offset, 0);

        pane.scroll_down(5);
        assert_eq!(pane.scroll_offset, 0);
    }

    #[test]
    fn copy_mode_scroll_with_scrollback() {
        let mut pane = create_test_pane();
        // 24行の画面に30行以上の出力を流し込み、スクロールバッファに行を蓄積する
        for i in 0..30 {
            pane.process_output(format!("line {i}\r\n").as_bytes());
        }
        pane.enter_copy_mode();

        pane.scroll_up(3);
        assert_eq!(pane.scroll_offset, 3);

        pane.scroll_down(1);
        assert_eq!(pane.scroll_offset, 2);

        // scroll_to_bottomでオフセットが0に戻る
        pane.scroll_to_bottom();
        assert_eq!(pane.scroll_offset, 0);
    }

    #[test]
    fn copy_mode_scroll_to_top() {
        let mut pane = create_test_pane();
        for i in 0..50 {
            pane.process_output(format!("line {i}\r\n").as_bytes());
        }
        pane.enter_copy_mode();

        pane.scroll_to_top();
        // スクロールオフセットがスクロールバッファの行数と一致する
        assert!(pane.scroll_offset > 0);
        let top = pane.scroll_offset;

        // さらにscroll_upしてもクランプされる
        pane.scroll_up(100);
        assert_eq!(pane.scroll_offset, top);
    }

    #[test]
    fn screen_text_returns_content() {
        let mut pane = create_test_pane();
        pane.process_output(b"hello");
        let text = pane.screen_text();
        assert!(text.contains("hello"));
    }

    #[test]
    fn selected_text_without_selection_returns_cursor_line() {
        let mut pane = create_test_pane();
        for i in 0..10 {
            pane.process_output(format!("line {i}\r\n").as_bytes());
        }
        pane.enter_copy_mode();
        // カーソルを3行目（line 2）に置く
        pane.copy_cursor = (2, 0);

        // 選択なしでyすると、画面全体ではなくカーソル行1行だけが返る
        let text = pane.selected_text();
        assert_eq!(text.trim_end(), "line 2");
        assert!(!text.contains("line 0"));
        assert!(!text.contains("line 3"));
    }

    #[test]
    fn selected_text_copies_selected_range_not_from_top() {
        let mut pane = create_test_pane();
        for i in 0..10 {
            pane.process_output(format!("line {i}\r\n").as_bytes());
        }
        pane.enter_copy_mode();

        // 画面行5（line 5）でアンカーを張り、画面行9（line 9）までカーソルを移動
        pane.copy_cursor = (5, 0);
        pane.toggle_selection(SelectionKind::Line);
        pane.copy_cursor = (9, 0);

        let text = pane.selected_text();
        let lines: Vec<&str> = text.lines().map(|l| l.trim_end()).collect();
        // 5-9行目が返り、先頭の0-4行目は含まれない
        assert_eq!(
            lines,
            vec!["line 5", "line 6", "line 7", "line 8", "line 9"]
        );
        assert!(!text.contains("line 0"));
        assert!(!text.contains("line 4"));
    }

    #[test]
    fn selected_text_single_line_selection() {
        let mut pane = create_test_pane();
        for i in 0..10 {
            pane.process_output(format!("line {i}\r\n").as_bytes());
        }
        pane.enter_copy_mode();

        // 同一行でアンカーとカーソルを置く（1行だけ選択）
        pane.copy_cursor = (6, 0);
        pane.toggle_selection(SelectionKind::Line);

        let text = pane.selected_text();
        assert_eq!(text.trim_end(), "line 6");
    }

    /// 1行目に指定テキストを書き込み、コピーモードに入ったテストペインを返す
    fn pane_with_line(text: &str) -> TerminalPane {
        let mut pane = create_test_pane();
        pane.process_output(text.as_bytes());
        pane.enter_copy_mode();
        pane
    }

    #[test]
    fn char_selection_copies_partial_line() {
        let mut pane = pane_with_line("foo bar baz");
        pane.copy_cursor = (0, 4);
        pane.toggle_selection(SelectionKind::Char);
        pane.copy_cursor = (0, 6);
        assert_eq!(pane.selected_text(), "bar");
    }

    #[test]
    fn char_selection_backward_from_anchor() {
        let mut pane = pane_with_line("foo bar baz");
        pane.copy_cursor = (0, 6);
        pane.toggle_selection(SelectionKind::Char);
        pane.copy_cursor = (0, 4);
        assert_eq!(pane.selected_text(), "bar");
    }

    #[test]
    fn char_selection_spans_lines() {
        let mut pane = pane_with_line("hello world\r\nsecond line");
        pane.copy_cursor = (0, 6);
        pane.toggle_selection(SelectionKind::Char);
        pane.copy_cursor = (1, 5);
        assert_eq!(pane.selected_text(), "world\nsecond");
    }

    #[test]
    fn char_selection_highlights_only_range() {
        let mut pane = pane_with_line("foo bar baz\r\nnext");
        pane.copy_cursor = (0, 4);
        pane.toggle_selection(SelectionKind::Char);
        pane.copy_cursor = (1, 1);
        assert!(!pane.is_cell_selected(0, 3));
        assert!(pane.is_cell_selected(0, 4));
        assert!(pane.is_cell_selected(0, 79));
        assert!(pane.is_cell_selected(1, 0));
        assert!(pane.is_cell_selected(1, 1));
        assert!(!pane.is_cell_selected(1, 2));
    }

    #[test]
    fn toggle_selection_switches_kind_and_clears() {
        let mut pane = pane_with_line("foo bar baz");
        pane.copy_cursor = (0, 4);
        pane.toggle_selection(SelectionKind::Char);
        pane.copy_cursor = (0, 6);
        // 別種別ならアンカーを保ったまま行単位に切り替わる
        pane.toggle_selection(SelectionKind::Line);
        assert!(pane.has_selection());
        assert_eq!(pane.selected_text().trim_end(), "foo bar baz");
        // 同じ種別をもう一度押すと解除
        pane.toggle_selection(SelectionKind::Line);
        assert!(!pane.has_selection());
    }

    #[test]
    fn word_motions_within_line() {
        let mut pane = pane_with_line("foo.bar  baz");
        pane.move_word_forward();
        assert_eq!(pane.copy_cursor, (0, 3)); // "."
        pane.move_word_forward();
        assert_eq!(pane.copy_cursor, (0, 4)); // "bar"
        pane.move_word_forward();
        assert_eq!(pane.copy_cursor, (0, 9)); // "baz"
        pane.move_word_end();
        assert_eq!(pane.copy_cursor, (0, 11));
        pane.move_word_backward();
        assert_eq!(pane.copy_cursor, (0, 9));
        pane.move_word_backward();
        assert_eq!(pane.copy_cursor, (0, 4));
    }

    #[test]
    fn word_motions_cross_lines() {
        let mut pane = pane_with_line("foo\r\n  bar");
        pane.move_word_forward();
        assert_eq!(pane.copy_cursor, (1, 2));
        pane.move_word_backward();
        assert_eq!(pane.copy_cursor, (0, 0));
    }

    #[test]
    fn word_motion_treats_wide_chars_as_word() {
        let mut pane = pane_with_line("ab 日本語 cd");
        pane.move_word_forward();
        assert_eq!(pane.copy_cursor, (0, 3));
        pane.move_word_end();
        // 「語」の前半セル（後半セルには止まらない）
        assert_eq!(pane.copy_cursor, (0, 7));
        pane.toggle_selection(SelectionKind::Char);
        pane.move_word_backward();
        assert_eq!(pane.selected_text(), "日本語");
    }

    #[test]
    fn line_start_end_motions() {
        let mut pane = pane_with_line("   indented text  ");
        pane.copy_cursor = (0, 8);
        pane.move_cursor_line_end();
        assert_eq!(pane.copy_cursor, (0, 15));
        pane.move_cursor_first_non_blank();
        assert_eq!(pane.copy_cursor, (0, 3));
        pane.move_cursor_line_start();
        assert_eq!(pane.copy_cursor, (0, 0));
    }

    #[test]
    fn horizontal_moves_skip_wide_char_continuation() {
        let mut pane = pane_with_line("a日本b");
        pane.move_cursor_right();
        assert_eq!(pane.copy_cursor, (0, 1)); // 「日」
        pane.move_cursor_right();
        assert_eq!(pane.copy_cursor, (0, 3)); // 「本」
        pane.move_cursor_right();
        assert_eq!(pane.copy_cursor, (0, 5)); // 「b」
        pane.move_cursor_left();
        assert_eq!(pane.copy_cursor, (0, 3));
        pane.move_cursor_left();
        assert_eq!(pane.copy_cursor, (0, 1));
        pane.move_cursor_left();
        assert_eq!(pane.copy_cursor, (0, 0));
    }

    #[test]
    fn char_selection_with_wide_chars() {
        let mut pane = pane_with_line("a日本b");
        pane.move_cursor_right();
        pane.toggle_selection(SelectionKind::Char);
        pane.move_cursor_right();
        assert_eq!(pane.selected_text(), "日本");
    }

    #[test]
    fn vertical_move_snaps_to_wide_char_head() {
        let mut pane = pane_with_line("abcd\r\n日本");
        pane.copy_cursor = (0, 3);
        pane.move_cursor_down();
        assert_eq!(pane.copy_cursor, (1, 2)); // 「本」の前半セル
    }
}
