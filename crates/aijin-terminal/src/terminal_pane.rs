//! PTYベースのターミナルペインの実装。
//! Paneトレイトを実装し、シェルの起動・入力・描画を管理する。

use std::any::Any;
use std::sync::mpsc;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;

use aijin_core::{Action, AppEvent, Pane, PaneId, PaneType};
use aijin_core::theme;

use crate::pty_handle::{PtyHandle, PtyMessage};
use crate::widget::TerminalWidget;

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
    /// テキスト選択の開始位置。スクロールに依存しない絶対行位置で保持する。
    /// 値は `scroll_offset * 1 + screen_row` ではなく、
    /// `(anchor_scroll_offset, anchor_screen_row)` として保持し、
    /// `anchor_absolute_row()` で論理行番号に変換する。
    selection_anchor: Option<(usize, u16)>,
}

impl TerminalPane {
    /// 新しいTerminalPaneを生成し、PTYプロセスを起動する。
    ///
    /// @param id - ペインID
    /// @param cols - 初期列数
    /// @param rows - 初期行数
    /// @param pty_sender - PTY出力の送信先チャネル
    /// @returns TerminalPaneインスタンス
    pub fn new(
        id: PaneId,
        cols: u16,
        rows: u16,
        pty_sender: mpsc::Sender<PtyMessage>,
    ) -> Self {
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
        let (pty, spawn_error) = match PtyHandle::spawn_with_command(id, cols, rows, pty_sender, cmd) {
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
    }

    /// コピーモードのカーソルを左に移動する
    pub fn move_cursor_left(&mut self) {
        self.copy_cursor.1 = self.copy_cursor.1.saturating_sub(1);
    }

    /// コピーモードのカーソルを右に移動する
    pub fn move_cursor_right(&mut self) {
        let max_col = self.last_size.0.saturating_sub(1);
        if self.copy_cursor.1 < max_col {
            self.copy_cursor.1 += 1;
        }
    }

    /// テキスト選択を開始/解除する。
    /// 未選択なら現在のカーソル位置をアンカーに設定、選択中なら解除する。
    pub fn toggle_selection(&mut self) {
        if self.selection_anchor.is_some() {
            self.selection_anchor = None;
        } else {
            // スクロールオフセットと画面行を保存して絶対位置を記録する
            self.selection_anchor = Some((self.scroll_offset, self.copy_cursor.0));
        }
    }

    /// 選択中かどうかを返す
    pub fn has_selection(&self) -> bool {
        self.selection_anchor.is_some()
    }

    /// 選択範囲のテキストを取得する。
    /// 選択なしなら画面全体のテキストを返す。
    ///
    /// @returns 選択範囲または画面全体のテキスト
    pub fn selected_text(&mut self) -> String {
        let Some((anchor_offset, anchor_row)) = self.selection_anchor else {
            return self.parser.screen().contents();
        };
        let anchor_logical = Self::logical_row(anchor_offset, anchor_row);
        let cursor_logical = Self::logical_row(self.scroll_offset, self.copy_cursor.0);
        let (start_logical, end_logical) = if anchor_logical <= cursor_logical {
            (anchor_logical, cursor_logical)
        } else {
            (cursor_logical, anchor_logical)
        };

        // 選択範囲全体が画面内に収まるscroll_offsetを計算する。
        // 論理行Lが画面行Rになるにはscroll_offset = R - L。
        // start_logicalを画面行0に配置するには offset = -start_logical。
        let needed_offset = (-start_logical).max(0) as usize;
        let original_offset = self.scroll_offset;
        self.parser.set_scrollback(needed_offset);

        let num_lines = (end_logical - start_logical) as u16;
        let screen = self.parser.screen();
        let cols = screen.size().1;
        let text = screen.contents_between(0, 0, num_lines, cols);

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

    /// 指定セルが選択範囲内かどうかを判定する。
    /// 行単位選択: 開始行から終了行まで各行全体がハイライトされる。
    /// アンカーとカーソルの絶対行位置を比較して判定する。
    pub fn is_cell_selected(&self, row: u16, _col: u16) -> bool {
        let Some((anchor_offset, anchor_row)) = self.selection_anchor else { return false };
        let anchor_logical = Self::logical_row(anchor_offset, anchor_row);
        let cursor_logical = Self::logical_row(self.scroll_offset, self.copy_cursor.0);
        let cell_logical = Self::logical_row(self.scroll_offset, row);

        let (start, end) = if anchor_logical <= cursor_logical {
            (anchor_logical, cursor_logical)
        } else {
            (cursor_logical, anchor_logical)
        };
        cell_logical >= start && cell_logical <= end
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
    pub fn render_content(&mut self, frame: &mut Frame, area: Rect) {
        // PTYスポーン失敗時はエラーメッセージを表示
        if let Some(ref err) = self.spawn_error {
            let msg = format!("Failed to start process: {err}");
            let text = ratatui::text::Text::styled(
                msg,
                Style::default().fg(theme::get().accent_error),
            );
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
            let cursor = self.copy_cursor;
            let anchor = self.selection_anchor;
            let scroll_offset = self.scroll_offset;
            widget = widget.with_copy_mode(cursor, move |row, _col| {
                let Some((anchor_offset, anchor_row)) = anchor else { return false };
                let anchor_logical = (anchor_row as isize) - (anchor_offset as isize);
                let cursor_logical = (cursor.0 as isize) - (scroll_offset as isize);
                let cell_logical = (row as isize) - (scroll_offset as isize);
                let (start, end) = if anchor_logical <= cursor_logical {
                    (anchor_logical, cursor_logical)
                } else {
                    (cursor_logical, anchor_logical)
                };
                cell_logical >= start && cell_logical <= end
            });
        }
        frame.render_widget(widget, area);

        // コピーモード時は最下行にインジケータを表示
        if self.copy_mode && area.height > 0 {
            let scroll_info = if self.scroll_offset > 0 {
                format!(" [COPY +{}] ", self.scroll_offset)
            } else {
                " [COPY] ".to_string()
            };
            let t = theme::get();
            let line = ratatui::text::Line::from(ratatui::text::Span::styled(
                scroll_info,
                Style::default().fg(t.text_on_accent).bg(t.accent_warning)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ));
            let indicator_area = Rect {
                x: area.x,
                y: area.y + area.height.saturating_sub(1),
                width: area.width,
                height: 1,
            };
            frame.render_widget(ratatui::widgets::Paragraph::new(line), indicator_area);
        }

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
    ///
    /// @param data - PTYからの出力バイト列
    pub fn process_output(&mut self, data: &[u8]) {
        self.parser.process(data);
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

    fn process_output(&mut self, data: &[u8]) {
        self.parser.process(data);
    }

    fn write_to_pty(&mut self, data: &[u8]) {
        if let Some(ref mut pty) = self.pty
            && pty.write(data).is_err()
        {
            self.pty_dead = true;
        }
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
    use aijin_core::PaneId;

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
}
