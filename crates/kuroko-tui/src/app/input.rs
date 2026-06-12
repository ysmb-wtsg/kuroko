//! App構造体のキーボード・マウス入力処理。
//! モード別のキーハンドリング、マウスイベント処理、ペーストイベント処理を担当する。

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};

use kuroko_core::{Action, AppEvent, Direction, FilePromptKind, Mode, PaneType};
use kuroko_agent::AgentPane;
use kuroko_filetree::FileTreePane;
use kuroko_terminal::TerminalPane;

use super::App;
use super::overlay::{CommandPalette, FilePrompt};

impl App {
    /// ペーストイベントを処理する。
    /// ブラケットペーストシーケンスで包んでPTYに送信し、
    /// PTY内のアプリが改行を「送信」ではなく「テキスト挿入」として扱えるようにする。
    pub(super) fn handle_paste(&self, text: String) -> Vec<Action> {
        let wants_raw = self.panes.get(&self.focused)
            .map(|p| p.wants_raw_input())
            .unwrap_or(true);

        if wants_raw {
            // ブラケットペーストシーケンスで包んでPTYに送信
            let mut data = Vec::with_capacity(text.len() + 12);
            data.extend_from_slice(b"\x1b[200~");
            data.extend_from_slice(text.as_bytes());
            data.extend_from_slice(b"\x1b[201~");
            vec![Action::PtyWrite {
                pane_id: self.focused,
                data,
            }]
        } else {
            // non-rawペインにはペーストイベントをAppEventとして渡す
            // （現状FileTree等はペースト非対応のため空）
            vec![]
        }
    }

    /// マウスイベントを処理する
    pub(super) fn handle_mouse(&mut self, mouse: MouseEvent) -> Vec<Action> {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                // フォーカス中のペインがターミナル/エージェントの場合、コピーモードに入ってスクロール
                let is_terminal_or_agent = self.panes.get(&self.focused)
                    .map(|p| matches!(p.pane_type(), PaneType::Terminal | PaneType::Agent))
                    .unwrap_or(false);
                if is_terminal_or_agent {
                    let in_copy_mode = self.panes.get(&self.focused)
                        .and_then(|p| {
                            p.as_any().downcast_ref::<TerminalPane>()
                                .map(|tp| tp.is_copy_mode())
                                .or_else(|| p.as_any().downcast_ref::<AgentPane>()
                                    .map(|ap| ap.is_copy_mode()))
                        })
                        .unwrap_or(false);
                    if !in_copy_mode {
                        // コピーモードに入ってからスクロール
                        self.dispatch_action(Action::EnterCopyMode);
                    }
                    self.copy_mode_scroll_up(3);
                }
                vec![]
            }
            MouseEventKind::ScrollDown => {
                // コピーモード中のみスクロールダウン
                let in_copy_mode = self.panes.get(&self.focused)
                    .and_then(|p| {
                        p.as_any().downcast_ref::<TerminalPane>()
                            .map(|tp| tp.is_copy_mode())
                            .or_else(|| p.as_any().downcast_ref::<AgentPane>()
                                .map(|ap| ap.is_copy_mode()))
                    })
                    .unwrap_or(false);
                if in_copy_mode {
                    self.copy_mode_scroll_down(3);
                    // 最新画面に到達したらコピーモードを終了
                    let at_bottom = self.panes.get(&self.focused)
                        .and_then(|p| {
                            p.as_any().downcast_ref::<TerminalPane>()
                                .map(|tp| tp.scroll_offset() == 0)
                                .or_else(|| p.as_any().downcast_ref::<AgentPane>()
                                    .map(|ap| ap.scroll_offset() == 0))
                        })
                        .unwrap_or(false);
                    if at_bottom {
                        return vec![Action::ExitCopyMode];
                    }
                }
                vec![]
            }
            MouseEventKind::Down(_) => {
                // クリック位置のペインにフォーカスを移動
                let pane_areas = self.layout.resolve(self.last_area);
                for (pane_id, area) in &pane_areas {
                    if mouse.column >= area.x
                        && mouse.column < area.x + area.width
                        && mouse.row >= area.y
                        && mouse.row < area.y + area.height
                    {
                        if *pane_id != self.focused {
                            return vec![Action::FocusPane(*pane_id)];
                        }
                        break;
                    }
                }
                vec![]
            }
            _ => vec![],
        }
    }

    /// ファイルプレビュー表示中のキー処理。
    /// j/kでスクロール、p/Escで閉じる。
    pub(super) fn handle_preview_key(&mut self, key: KeyEvent) -> Vec<Action> {
        if key.kind != KeyEventKind::Press {
            return vec![];
        }
        match key.code {
            KeyCode::Char('p') | KeyCode::Esc | KeyCode::Char('q') => {
                self.overlay.file_preview = None;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(ref mut preview) = self.overlay.file_preview {
                    preview.scroll = preview.scroll.saturating_add(1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(ref mut preview) = self.overlay.file_preview {
                    preview.scroll = preview.scroll.saturating_sub(1);
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref mut preview) = self.overlay.file_preview {
                    preview.scroll = preview.scroll.saturating_add(20);
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref mut preview) = self.overlay.file_preview {
                    preview.scroll = preview.scroll.saturating_sub(20);
                }
            }
            KeyCode::Char('g') => {
                if let Some(ref mut preview) = self.overlay.file_preview {
                    preview.scroll = 0;
                }
            }
            KeyCode::Char('G') => {
                if let Some(ref mut preview) = self.overlay.file_preview {
                    preview.scroll = preview.lines.len().saturating_sub(1);
                }
            }
            _ => {}
        }
        vec![]
    }

    /// ファイル操作プロンプト表示中のキー処理。
    /// Enter で操作実行、Esc でキャンセル。
    pub(super) fn handle_file_prompt_key(&mut self, key: KeyEvent) -> Vec<Action> {
        if key.kind != KeyEventKind::Press {
            return vec![];
        }
        match key.code {
            KeyCode::Esc => {
                self.overlay.file_prompt = None;
            }
            KeyCode::Enter => {
                if let Some(prompt) = self.overlay.file_prompt.take() {
                    self.execute_file_prompt(prompt);
                }
            }
            KeyCode::Char(c) => {
                if let Some(ref mut prompt) = self.overlay.file_prompt {
                    // Delete確認時は 'y' のみ受付
                    if matches!(prompt.kind, FilePromptKind::Delete { .. }) {
                        if c == 'y' {
                            if let Some(prompt) = self.overlay.file_prompt.take() {
                                self.execute_file_prompt(prompt);
                            }
                        } else if c == 'n' {
                            self.overlay.file_prompt = None;
                        }
                    } else {
                        prompt.input.push(c);
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(ref mut prompt) = self.overlay.file_prompt
                    && !matches!(prompt.kind, FilePromptKind::Delete { .. }) {
                        prompt.input.pop();
                    }
            }
            _ => {}
        }
        vec![]
    }

    /// ヘルプ表示中のキー処理。Esc/q で閉じる。
    pub(super) fn handle_help_key(&mut self, key: KeyEvent) -> Vec<Action> {
        if key.kind != KeyEventKind::Press {
            return vec![];
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.overlay.help_visible = false;
            }
            _ => {}
        }
        vec![]
    }

    /// ファイル詳細表示中のキー処理。Esc/i/q で閉じる。
    pub(super) fn handle_file_info_key(&mut self, key: KeyEvent) -> Vec<Action> {
        if key.kind != KeyEventKind::Press {
            return vec![];
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('i') | KeyCode::Char('q') => {
                self.overlay.file_info = None;
            }
            _ => {}
        }
        vec![]
    }

    /// SELECTモードのキー処理。
    /// FileTreeペインにフォーカスがある場合のみ有効。
    pub(super) fn handle_select_key(&mut self, key: KeyEvent) -> Vec<Action> {
        if key.kind != KeyEventKind::Press {
            return vec![];
        }

        // FileTree以外にフォーカスがある場合はInsertに戻す
        let is_filetree = self.panes.get(&self.focused)
            .map(|p| p.pane_type() == PaneType::FileTree)
            .unwrap_or(false);
        if !is_filetree {
            return vec![Action::SetMode(Mode::Insert)];
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(pane) = self.panes.get_mut(&self.focused)
                    && let Some(ft) = pane.as_any_mut().downcast_mut::<FileTreePane>() {
                        ft.move_down();
                    }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(pane) = self.panes.get_mut(&self.focused)
                    && let Some(ft) = pane.as_any_mut().downcast_mut::<FileTreePane>() {
                        ft.move_up();
                    }
            }
            KeyCode::Char(' ') => {
                if let Some(pane) = self.panes.get_mut(&self.focused)
                    && let Some(ft) = pane.as_any_mut().downcast_mut::<FileTreePane>() {
                        ft.toggle_selection();
                    }
            }
            KeyCode::Char('d') => {
                let paths = self.get_filetree_selected_paths();
                if !paths.is_empty() {
                    self.overlay.file_prompt = Some(FilePrompt {
                        kind: FilePromptKind::Delete { paths },
                        input: String::new(),
                    });
                }
            }
            KeyCode::Char('y') => {
                let paths = self.get_filetree_selected_paths();
                if !paths.is_empty() {
                    let text: String = paths.iter()
                        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.copy_to_clipboard(&text);
                }
            }
            KeyCode::Char('Y') => {
                let paths = self.get_filetree_selected_paths();
                if !paths.is_empty() {
                    let text: String = paths.iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.copy_to_clipboard(&text);
                }
            }
            KeyCode::Char('v') | KeyCode::Esc => {
                if let Some(pane) = self.panes.get_mut(&self.focused)
                    && let Some(ft) = pane.as_any_mut().downcast_mut::<FileTreePane>() {
                        ft.clear_selections();
                    }
                return vec![Action::SetMode(Mode::Insert)];
            }
            _ => {}
        }
        vec![]
    }

    /// リネームモードのキー処理。
    /// `overlay.renaming_bottom_tab` に応じてメインタブまたはボトムタブのリネームを行う。
    pub(super) fn handle_rename_key(&mut self, key: KeyEvent) -> Vec<Action> {
        match key.code {
            KeyCode::Esc => {
                self.overlay.rename_input = None;
                self.overlay.renaming_bottom_tab = false;
                vec![]
            }
            KeyCode::Enter => {
                let name = self.overlay.rename_input.take().unwrap_or_default();
                let is_bottom = self.overlay.renaming_bottom_tab;
                self.overlay.renaming_bottom_tab = false;
                if is_bottom {
                    vec![Action::RenameTerminalTab(name)]
                } else {
                    vec![Action::RenameTab(name)]
                }
            }
            KeyCode::Char(c) => {
                if let Some(ref mut input) = self.overlay.rename_input {
                    input.push(c);
                }
                vec![]
            }
            KeyCode::Backspace => {
                if let Some(ref mut input) = self.overlay.rename_input {
                    input.pop();
                }
                vec![]
            }
            _ => vec![],
        }
    }

    /// Insertモードのキー処理。
    /// PTYペインにはバイト列を転送し、non-rawペイン（FileTree等）にはAppEventとして渡す。
    pub(super) fn handle_insert_key(&mut self, key: KeyEvent) -> Vec<Action> {
        // Esc でNormalモードに切り替え
        if key.code == KeyCode::Esc {
            return vec![Action::SetMode(Mode::Normal)];
        }

        // non-rawペイン（FileTree等）にはAppEventとして渡す
        let wants_raw = self.panes.get(&self.focused)
            .map(|p| p.wants_raw_input())
            .unwrap_or(true);

        if !wants_raw {
            let event = AppEvent::Key(key);
            if let Some(pane) = self.panes.get_mut(&self.focused) {
                return pane.handle_event(&event);
            }
            return vec![];
        }

        if let Some(data) = super::key_to_bytes(&key) {
            vec![Action::PtyWrite {
                pane_id: self.focused,
                data,
            }]
        } else {
            vec![]
        }
    }

    /// Normalモードのキー処理。
    /// 非rawペイン（FileTree等）にフォーカス中は、グローバル予約キー以外を
    /// ペインに直接ルーティングする（Insertモードを経由しない）。
    pub(super) fn handle_normal_key(&mut self, key: KeyEvent) -> Vec<Action> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // Ctrl+hjkl はペイン種別に関わらず常にフォーカス移動
        if ctrl {
            match key.code {
                KeyCode::Char('h') => return vec![Action::FocusDirection(Direction::Left)],
                KeyCode::Char('j') => return vec![Action::FocusDirection(Direction::Down)],
                KeyCode::Char('k') => return vec![Action::FocusDirection(Direction::Up)],
                KeyCode::Char('l') => return vec![Action::FocusDirection(Direction::Right)],
                _ => {}
            }
        }

        let wants_raw = self.panes.get(&self.focused)
            .map(|p| p.wants_raw_input())
            .unwrap_or(true);

        if !wants_raw {
            // グローバル予約キー（フォーカス移動・パネルトグル・パレット・終了）
            match key.code {
                KeyCode::Tab => return vec![Action::FocusNext],
                KeyCode::BackTab => return vec![Action::FocusPrev],
                KeyCode::Char('t') => return vec![Action::ToggleTerminal],
                KeyCode::Char('f') => return vec![Action::ToggleFileTree],
                KeyCode::Char('g') => return vec![Action::ToggleGitPanel],
                KeyCode::Char(':') => {
                    self.overlay.command_palette = Some(CommandPalette::new());
                    return vec![];
                }
                KeyCode::Char('q') => return vec![Action::Quit],
                _ => {}
            }
            // 予約キー以外はペインへ直接渡す
            let event = AppEvent::Key(key);
            if let Some(pane) = self.panes.get_mut(&self.focused) {
                return pane.handle_event(&event);
            }
            return vec![];
        }

        let is_main_tab = self.main_tabs.contains(&self.focused);
        let is_bottom_terminal = self.is_bottom_terminal_focused();

        match key.code {
            // Insertモードに切り替え
            KeyCode::Char('i') => vec![Action::SetMode(Mode::Insert)],

            // 方向フォーカス移動
            KeyCode::Char('h') => vec![Action::FocusDirection(Direction::Left)],
            KeyCode::Char('j') => vec![Action::FocusDirection(Direction::Down)],
            KeyCode::Char('k') => vec![Action::FocusDirection(Direction::Up)],
            KeyCode::Char('l') => vec![Action::FocusDirection(Direction::Right)],

            // Tab でフォーカス順送り
            KeyCode::Tab => vec![Action::FocusNext],
            KeyCode::BackTab => vec![Action::FocusPrev],

            // ペインリサイズ（Shift + hjkl に相当するキー）
            KeyCode::Char('H') => vec![Action::ResizePane { direction: Direction::Left, amount: 2 }],
            KeyCode::Char('J') => vec![Action::ResizePane { direction: Direction::Down, amount: 2 }],
            KeyCode::Char('K') => vec![Action::ResizePane { direction: Direction::Up, amount: 2 }],
            KeyCode::Char('L') => vec![Action::ResizePane { direction: Direction::Right, amount: 2 }],

            // サイドパネルトグル
            KeyCode::Char('t') => vec![Action::ToggleTerminal],
            KeyCode::Char('f') => vec![Action::ToggleFileTree],
            KeyCode::Char('g') => vec![Action::ToggleGitPanel],

            // メインタブ操作（メインタブにフォーカスがある場合のみ有効）
            KeyCode::Char('n') if is_main_tab => vec![Action::NewTab],
            KeyCode::Char('x') | KeyCode::Char('w') if is_main_tab => vec![Action::CloseTab],
            KeyCode::Char('r') if is_main_tab => {
                self.overlay.rename_input = Some(String::new());
                self.overlay.renaming_bottom_tab = false;
                vec![]
            }
            KeyCode::Char(']') if is_main_tab => vec![Action::NextTab],
            KeyCode::Char('[') if is_main_tab => vec![Action::PrevTab],
            KeyCode::Char(c @ '1'..='9') if is_main_tab => vec![Action::SelectTab((c as usize) - ('1' as usize))],

            // サイドターミナルタブ操作
            KeyCode::Char('n') if is_bottom_terminal => vec![Action::NewTerminalTab],
            KeyCode::Char('x') | KeyCode::Char('w') if is_bottom_terminal => vec![Action::CloseTerminalTab],
            KeyCode::Char('r') if is_bottom_terminal => {
                self.overlay.rename_input = Some(String::new());
                self.overlay.renaming_bottom_tab = true;
                vec![]
            }
            KeyCode::Char(']') if is_bottom_terminal => vec![Action::NextTerminalTab],
            KeyCode::Char('[') if is_bottom_terminal => vec![Action::PrevTerminalTab],
            KeyCode::Char(c @ '1'..='9') if is_bottom_terminal => vec![Action::SelectTerminalTab((c as usize) - ('1' as usize))],

            // コマンドパレット
            KeyCode::Char(':') => {
                self.overlay.command_palette = Some(CommandPalette::new());
                vec![]
            }

            // コピーモード（ターミナル/エージェントペインにフォーカス中のみ）
            KeyCode::Enter => {
                let is_terminal_or_agent = self.panes.get(&self.focused)
                    .map(|p| matches!(p.pane_type(), PaneType::Terminal | PaneType::Agent))
                    .unwrap_or(false);
                if is_terminal_or_agent {
                    vec![Action::EnterCopyMode]
                } else {
                    vec![]
                }
            }

            // 終了
            KeyCode::Char('q') => vec![Action::Quit],
            _ => vec![],
        }
    }

    /// コピーモード中のキー処理。
    /// hjkl でカーソル移動、Ctrl-d/u でハーフページスクロール、
    /// v で選択開始/解除、y で選択テキストをコピー、q/Esc で終了。
    pub(super) fn handle_copy_mode_key(&mut self, key: KeyEvent) -> Vec<Action> {
        if key.kind != KeyEventKind::Press {
            return vec![];
        }

        match key.code {
            // カーソル移動（兼スクロール：画面端に達すると自動スクロール）
            KeyCode::Char('j') | KeyCode::Down => {
                self.copy_mode_cursor_down();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.copy_mode_cursor_up();
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.copy_mode_cursor_left();
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.copy_mode_cursor_right();
            }
            // ハーフページスクロール
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (self.last_area.height / 2).max(1) as usize;
                self.copy_mode_scroll_down(half);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (self.last_area.height / 2).max(1) as usize;
                self.copy_mode_scroll_up(half);
            }
            // 先頭/末尾ジャンプ
            KeyCode::Char('g') => {
                self.copy_mode_scroll_to_top();
            }
            KeyCode::Char('G') => {
                self.copy_mode_scroll_to_bottom();
            }
            // テキスト選択の開始/解除
            KeyCode::Char('v') => {
                self.copy_mode_toggle_selection();
            }
            // コピー（選択範囲がなければ画面全体）
            KeyCode::Char('y') => {
                if let Some(text) = self.copy_mode_selected_text() {
                    let line_count = text.lines().count();
                    return vec![
                        Action::CopyToClipboard(text),
                        Action::Notify(format!("Copied {line_count} lines")),
                        Action::ExitCopyMode,
                    ];
                }
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                return vec![Action::ExitCopyMode];
            }
            _ => {}
        }
        vec![]
    }

    /// コピーモード: 上方向（過去）へスクロールする。
    /// フォーカス中のペインがTerminalPaneまたはAgentPaneの場合に委譲する。
    fn copy_mode_scroll_up(&mut self, lines: usize) {
        if let Some(pane) = self.panes.get_mut(&self.focused) {
            if let Some(tp) = pane.as_any_mut().downcast_mut::<TerminalPane>() {
                tp.scroll_up(lines);
            } else if let Some(ap) = pane.as_any_mut().downcast_mut::<AgentPane>() {
                ap.scroll_up(lines);
            }
        }
    }

    /// コピーモード: 下方向（最新）へスクロールする。
    /// フォーカス中のペインがTerminalPaneまたはAgentPaneの場合に委譲する。
    fn copy_mode_scroll_down(&mut self, lines: usize) {
        if let Some(pane) = self.panes.get_mut(&self.focused) {
            if let Some(tp) = pane.as_any_mut().downcast_mut::<TerminalPane>() {
                tp.scroll_down(lines);
            } else if let Some(ap) = pane.as_any_mut().downcast_mut::<AgentPane>() {
                ap.scroll_down(lines);
            }
        }
    }

    /// コピーモード: スクロールバッファの先頭まで移動する。
    fn copy_mode_scroll_to_top(&mut self) {
        if let Some(pane) = self.panes.get_mut(&self.focused) {
            if let Some(tp) = pane.as_any_mut().downcast_mut::<TerminalPane>() {
                tp.scroll_to_top();
            } else if let Some(ap) = pane.as_any_mut().downcast_mut::<AgentPane>() {
                ap.scroll_to_top();
            }
        }
    }

    /// コピーモード: スクロールオフセットを0（最新画面）にリセットする。
    fn copy_mode_scroll_to_bottom(&mut self) {
        if let Some(pane) = self.panes.get_mut(&self.focused) {
            if let Some(tp) = pane.as_any_mut().downcast_mut::<TerminalPane>() {
                tp.scroll_to_bottom();
            } else if let Some(ap) = pane.as_any_mut().downcast_mut::<AgentPane>() {
                ap.scroll_to_bottom();
            }
        }
    }

    /// コピーモード: カーソルを上に移動する
    fn copy_mode_cursor_up(&mut self) {
        if let Some(pane) = self.panes.get_mut(&self.focused) {
            if let Some(tp) = pane.as_any_mut().downcast_mut::<TerminalPane>() {
                tp.move_cursor_up();
            } else if let Some(ap) = pane.as_any_mut().downcast_mut::<AgentPane>() {
                ap.move_cursor_up();
            }
        }
    }

    /// コピーモード: カーソルを下に移動する
    fn copy_mode_cursor_down(&mut self) {
        if let Some(pane) = self.panes.get_mut(&self.focused) {
            if let Some(tp) = pane.as_any_mut().downcast_mut::<TerminalPane>() {
                tp.move_cursor_down();
            } else if let Some(ap) = pane.as_any_mut().downcast_mut::<AgentPane>() {
                ap.move_cursor_down();
            }
        }
    }

    /// コピーモード: カーソルを左に移動する
    fn copy_mode_cursor_left(&mut self) {
        if let Some(pane) = self.panes.get_mut(&self.focused) {
            if let Some(tp) = pane.as_any_mut().downcast_mut::<TerminalPane>() {
                tp.move_cursor_left();
            } else if let Some(ap) = pane.as_any_mut().downcast_mut::<AgentPane>() {
                ap.move_cursor_left();
            }
        }
    }

    /// コピーモード: カーソルを右に移動する
    fn copy_mode_cursor_right(&mut self) {
        if let Some(pane) = self.panes.get_mut(&self.focused) {
            if let Some(tp) = pane.as_any_mut().downcast_mut::<TerminalPane>() {
                tp.move_cursor_right();
            } else if let Some(ap) = pane.as_any_mut().downcast_mut::<AgentPane>() {
                ap.move_cursor_right();
            }
        }
    }

    /// コピーモード: テキスト選択の開始/解除をトグルする
    fn copy_mode_toggle_selection(&mut self) {
        if let Some(pane) = self.panes.get_mut(&self.focused) {
            if let Some(tp) = pane.as_any_mut().downcast_mut::<TerminalPane>() {
                tp.toggle_selection();
            } else if let Some(ap) = pane.as_any_mut().downcast_mut::<AgentPane>() {
                ap.toggle_selection();
            }
        }
    }

    /// コピーモード: 選択範囲のテキストを取得する（選択なしなら画面全体）
    fn copy_mode_selected_text(&mut self) -> Option<String> {
        let pane = self.panes.get_mut(&self.focused)?;
        pane.as_any_mut().downcast_mut::<TerminalPane>()
            .map(|tp| tp.selected_text())
            .or_else(|| pane.as_any_mut().downcast_mut::<AgentPane>()
                .map(|ap| ap.selected_text()))
    }

    /// コマンドパレット表示中のキー処理。
    /// 文字入力でフィルタリング、j/k/矢印で候補選択、Enterで実行、Esc/Ctrl-cで閉じる。
    pub(super) fn handle_command_palette_key(&mut self, key: KeyEvent) -> Vec<Action> {
        if key.kind != KeyEventKind::Press {
            return vec![];
        }

        // Ctrl-c で閉じる
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.overlay.command_palette = None;
            return vec![];
        }

        match key.code {
            KeyCode::Esc => {
                self.overlay.command_palette = None;
                vec![]
            }
            KeyCode::Enter => {
                let action = self.overlay.command_palette
                    .as_ref()
                    .and_then(|cp| cp.selected_action());
                self.overlay.command_palette = None;
                match action {
                    Some(a) => vec![a],
                    None => vec![],
                }
            }
            KeyCode::Down => {
                if let Some(ref mut cp) = self.overlay.command_palette {
                    cp.move_down();
                }
                vec![]
            }
            KeyCode::Up => {
                if let Some(ref mut cp) = self.overlay.command_palette {
                    cp.move_up();
                }
                vec![]
            }
            KeyCode::Char(c) => {
                if let Some(ref mut cp) = self.overlay.command_palette {
                    cp.input.push(c);
                    cp.update_filter();
                }
                vec![]
            }
            KeyCode::Backspace => {
                if let Some(ref mut cp) = self.overlay.command_palette {
                    cp.input.pop();
                    cp.update_filter();
                }
                vec![]
            }
            _ => vec![],
        }
    }
}
