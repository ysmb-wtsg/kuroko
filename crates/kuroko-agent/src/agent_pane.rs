//! AIエージェントペインの実装。
//! TerminalPaneに委譲し、エージェント固有のステータス管理とPaneTypeを提供する。

use std::any::Any;
use std::sync::mpsc;
use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::Rect;

use kuroko_core::{Action, AppEvent, Pane, PaneId, PaneType};
use kuroko_terminal::TerminalPane;
use kuroko_terminal::pty_handle::PtyMessage;

use crate::provider::{AgentProvider, BuiltinProvider};
use crate::status::{ActivityTracker, AgentStatus, IdleNotifier};

/// AIエージェントペイン。
/// 内部にTerminalPaneを保持し、PTY操作を委譲する。
/// ボーダースタイルとPaneType、ステータス表示がエージェント固有。
pub struct AgentPane {
    /// 委譲先のターミナルペイン
    inner: TerminalPane,
    /// 出力活動からステータスを推定するトラッカー
    activity: ActivityTracker,
    /// Working→Idle 遷移を検出してデスクトップ通知を発火させるトラッカー
    idle_notifier: IdleNotifier,
    /// エージェント名（"Agent:" プレフィックスを含まない表示名。通知本文に使う）
    name: String,
}

impl AgentPane {
    /// 指定プロバイダーでエージェントペインを生成する。
    ///
    /// @param id - ペインID
    /// @param provider - エージェントプロバイダー
    /// @param cols - 初期列数
    /// @param rows - 初期行数
    /// @param pty_sender - PTY出力の送信先チャネル
    /// @returns AgentPaneインスタンス
    pub fn new(
        id: PaneId,
        provider: &BuiltinProvider,
        cols: u16,
        rows: u16,
        pty_sender: mpsc::Sender<PtyMessage>,
    ) -> Self {
        let title = provider.title();
        let mut cmd = provider.command();
        cmd.cwd(std::env::current_dir().unwrap_or_else(|_| "/".into()));

        Self {
            inner: TerminalPane::from_command(id, &title, cols, rows, pty_sender, cmd),
            activity: ActivityTracker::new(),
            idle_notifier: IdleNotifier::new(),
            name: provider.name().to_string(),
        }
    }

    /// PTYにバイト列を書き込む
    pub fn write_to_pty(&mut self, data: &[u8]) {
        self.inner.write_to_pty(data);
    }

    /// PTYプロセスが終了済みかどうかを設定し、ステータスをExitedに更新する
    pub fn set_pty_dead(&mut self) {
        self.inner.set_pty_dead();
        self.activity.mark_exited();
    }

    /// PTYが使用不能かどうかを返す
    pub fn is_pty_dead(&self) -> bool {
        self.inner.is_pty_dead()
    }

    /// コピーモードに入る。内部のTerminalPaneに委譲する。
    pub fn enter_copy_mode(&mut self) {
        self.inner.enter_copy_mode();
    }

    /// コピーモードを終了する。内部のTerminalPaneに委譲する。
    pub fn exit_copy_mode(&mut self) {
        self.inner.exit_copy_mode();
    }

    /// コピーモード中かどうかを返す。内部のTerminalPaneに委譲する。
    ///
    /// @returns コピーモードが有効ならtrue
    pub fn is_copy_mode(&self) -> bool {
        self.inner.is_copy_mode()
    }

    /// 現在のスクロールオフセットを返す。内部のTerminalPaneに委譲する。
    /// 0が最新画面、値が大きいほど過去の行を表示中。
    ///
    /// @returns スクロールオフセット（行数）
    pub fn scroll_offset(&self) -> usize {
        self.inner.scroll_offset()
    }

    /// スクロールバッファを上方向（過去）にスクロールする。
    ///
    /// @param lines - スクロールする行数
    pub fn scroll_up(&mut self, lines: usize) {
        self.inner.scroll_up(lines);
    }

    /// スクロールバッファを下方向（最新）にスクロールする。
    ///
    /// @param lines - スクロールする行数
    pub fn scroll_down(&mut self, lines: usize) {
        self.inner.scroll_down(lines);
    }

    /// 最大スクロールオフセット（スクロールバッファの先頭）まで移動する。
    pub fn scroll_to_top(&mut self) {
        self.inner.scroll_to_top();
    }

    /// スクロールオフセットを0（最新画面）にリセットする。
    pub fn scroll_to_bottom(&mut self) {
        self.inner.scroll_to_bottom();
    }

    /// 現在表示中の画面テキストを返す（クリップボードコピー用）
    pub fn screen_text(&self) -> String {
        self.inner.screen_text()
    }

    /// カーソルを上に移動する
    pub fn move_cursor_up(&mut self) {
        self.inner.move_cursor_up();
    }
    /// カーソルを下に移動する
    pub fn move_cursor_down(&mut self) {
        self.inner.move_cursor_down();
    }
    /// カーソルを左に移動する
    pub fn move_cursor_left(&mut self) {
        self.inner.move_cursor_left();
    }
    /// カーソルを右に移動する
    pub fn move_cursor_right(&mut self) {
        self.inner.move_cursor_right();
    }
    /// テキスト選択の開始/解除をトグルする
    pub fn toggle_selection(&mut self) {
        self.inner.toggle_selection();
    }
    /// 選択範囲のテキストを返す（選択なしならカーソル行）
    pub fn selected_text(&mut self) -> String {
        self.inner.selected_text()
    }
    /// 選択中かどうかを返す
    pub fn has_selection(&self) -> bool {
        self.inner.has_selection()
    }

    /// 現在のエージェントステータスを返す。
    /// 最後の出力からの経過時間で作業中/入力待ちを推定する。
    pub fn status(&self) -> AgentStatus {
        self.activity.status_at(Instant::now())
    }

    /// エージェント名を返す。タブ用の `title()`（"Agent:" プレフィックス付き）と異なり、
    /// 通知本文向けにプレフィックスなしの名前（例: "Claude Code"）を返す。
    ///
    /// @returns エージェント名
    pub fn agent_name(&self) -> &str {
        &self.name
    }

    /// 入力待ちへの遷移（Working→Idle）を検出する。通知すべき瞬間なら true を返す。
    /// 状態は時間経過で変わるため、メインループから毎フレーム呼び出す想定。
    ///
    /// @returns エージェントが処理を終えて入力待ちになった瞬間なら true
    pub fn poll_idle_notification(&mut self) -> bool {
        let status = self.activity.status_at(Instant::now());
        self.idle_notifier.observe(status)
    }
}

impl Pane for AgentPane {
    fn id(&self) -> PaneId {
        self.inner.id()
    }

    fn title(&self) -> &str {
        self.inner.title()
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _focused: bool) {
        // ボーダーは描画しない。フォーカスの有無で描画領域を変えると
        // PTYリサイズが発生して出力が再フローするため、常に全領域に描く。
        // ステータス（Working/Exited）はタブバー・ステータスバー側で表示する。
        self.inner.render_content(frame, area);
    }

    fn handle_event(&mut self, _event: &AppEvent) -> Vec<Action> {
        vec![]
    }

    fn wants_raw_input(&self) -> bool {
        true
    }

    fn pane_type(&self) -> PaneType {
        PaneType::Agent
    }

    fn cursor_position(&self, area: Rect) -> Option<(u16, u16)> {
        self.inner.compute_cursor_position(area)
    }

    fn process_output(&mut self, data: &[u8]) {
        self.inner.process_output(data);
        // 出力が届いている間はWorking、途絶えたらIdleと推定するため受信時刻を記録する
        self.activity.record_output(Instant::now());
    }

    fn write_to_pty(&mut self, data: &[u8]) {
        self.inner.write_to_pty(data);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
