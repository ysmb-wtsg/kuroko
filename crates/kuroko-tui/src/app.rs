//! アプリケーションのメインループと状態管理。
//! イベントの受信・ディスパッチ、直通/グローバルレイヤーの入力ルーティング、描画を統括する。

mod file_ops;
mod input;
mod overlay;
mod render;
pub(crate) mod session;
mod tab_manager;

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::supports_keyboard_enhancement;
use ratatui::layout::Rect;

use kuroko_agent::{AgentPane, BuiltinProvider};
use kuroko_core::layout::SplitDirection;
use kuroko_core::{Action, Direction, FilePromptKind, LayoutNode, Pane, PaneId, SideContent};
use kuroko_filetree::FileTreePane;
use kuroko_lua::{DEFAULT_TOGGLE_KEY, KeymapContext, LuaRuntime, SharedKeymapRegistry};
use kuroko_terminal::TerminalPane;
use kuroko_terminal::pty_handle::PtyMessage;

use overlay::{FileInfo, FilePreview, FilePrompt, MessageLevel, OverlayState};
use tab_manager::TabManager;

/// アプリケーションのメイン構造体。
/// パネルスロット方式でレイアウトを管理し、各パネルのトグル表示を制御する。
pub struct App {
    /// 全ペインの管理マップ（非表示ペインも含む）
    panes: HashMap<PaneId, Box<dyn Pane>>,
    /// レイアウトツリー（パネル表示状態から派生）
    layout: LayoutNode,
    /// 現在フォーカスされているペインのID
    focused: PaneId,
    /// グローバルレイヤー中かどうか（false = 全キーがフォーカス中ペインへ直通）
    global_layer: bool,
    /// 次に発行するペインIDのカウンター
    next_pane_id: u64,
    /// PTY出力の受信チャネル
    pty_rx: mpsc::Receiver<PtyMessage>,
    /// PTY出力の送信チャネル（新しいペイン生成時にクローンする）
    pty_tx: mpsc::Sender<PtyMessage>,
    /// Luaランタイム（init.lua読み込みやコールバック実行に使用）
    lua_runtime: Option<LuaRuntime>,
    /// Luaからのアクション受信チャネル
    lua_action_rx: mpsc::Receiver<Action>,
    /// Luaから登録されたカスタムキーマップ
    keymap_registry: Option<SharedKeymapRegistry>,
    /// オーバーレイUI（プレビュー/プロンプト/情報/メッセージ/リネーム）の集約状態
    overlay: OverlayState,
    /// アプリケーション終了フラグ
    should_quit: bool,
    /// 直前のフレームの描画領域（方向フォーカス計算用）
    last_area: Rect,

    // --- メインタブ ---
    /// メインペインのタブ管理
    main_tabs: TabManager,
    /// 新規タブ生成時に使用するエージェントプロバイダー
    tab_provider: BuiltinProvider,

    // --- サイドパネル（右） ---
    /// サイドパネルに表示中のサブパネル（Noneなら非表示）
    side_content: Option<SideContent>,
    /// サイドパネルの幅比率（0.0〜1.0、右側の割合）
    side_ratio: f32,
    /// ファイルツリーのペインID（初回トグル時に生成）
    file_tree_id: Option<PaneId>,
    /// GitパネルのペインID（初回トグル時に生成）
    git_panel_id: Option<PaneId>,
    /// Gitパネルで起動するツール名（lazygit, tig, gitui等）
    git_tool: String,

    // --- ボトムターミナル ---
    /// ボトムターミナルの表示状態
    bottom_visible: bool,
    /// ボトムターミナルの高さ比率（メイン側の割合、0.0〜1.0）
    bottom_ratio: f32,
    /// ボトムターミナルのタブ管理
    bottom_terminal_tabs: TabManager,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// 新しいAppインスタンスを生成する。
    /// Lua設定を先に読み込み、`krk.opt.main_pane` に応じたメインペインを配置する。
    pub fn new() -> Self {
        let saved_session = session::load();
        let (pty_tx, pty_rx) = mpsc::channel();
        let (lua_action_tx, lua_action_rx) = mpsc::channel();

        // Luaランタイムの初期化（ペイン生成前に行い、設定を読み込む）
        let lua_runtime = LuaRuntime::new(lua_action_tx).ok();
        let keymap_registry = lua_runtime.as_ref().map(|lua| lua.keymap_registry());

        // init.luaの読み込み（存在すれば）。エラーは起動後に通知する。
        let mut lua_init_error: Option<String> = None;
        if let Some(ref lua) = lua_runtime {
            let config_path = dirs_config_path().join("init.lua");
            if config_path.exists()
                && let Err(e) = lua.exec_file(&config_path)
            {
                lua_init_error = Some(format!("init.lua: {e}"));
            }
        }

        // Gitパネルで起動するツール（デフォルト: "lazygit"）
        let git_tool = lua_runtime
            .as_ref()
            .and_then(|lua| lua.get_opt_string("git_tool"))
            .unwrap_or_else(|| "lazygit".to_string());

        // メインペインの種類を設定から決定する（デフォルト: "claude-code"）
        let main_pane_setting = lua_runtime
            .as_ref()
            .and_then(|lua| lua.get_opt_string("main_pane"))
            .unwrap_or_else(|| "claude-code".to_string());

        let main_id = PaneId(0);
        let tab_provider = match main_pane_setting.as_str() {
            "codex" => BuiltinProvider::Codex,
            _ => BuiltinProvider::ClaudeCode,
        };
        let main_pane: Box<dyn Pane> = match main_pane_setting.as_str() {
            "terminal" => Box::new(TerminalPane::new(main_id, 80, 24, pty_tx.clone())),
            _ => Box::new(AgentPane::new(
                main_id,
                &tab_provider,
                80,
                24,
                pty_tx.clone(),
            )),
        };

        let mut panes: HashMap<PaneId, Box<dyn Pane>> = HashMap::new();
        panes.insert(main_id, main_pane);

        let mut app = Self {
            panes,
            layout: LayoutNode::Leaf(main_id),
            focused: main_id,
            global_layer: false,
            next_pane_id: 1,
            pty_rx,
            pty_tx,
            lua_runtime,
            lua_action_rx,
            keymap_registry,
            overlay: OverlayState::new(),
            should_quit: false,
            last_area: Rect::default(),
            main_tabs: TabManager::with_initial(main_id),
            tab_provider,
            side_content: None,
            side_ratio: saved_session.side_ratio,
            file_tree_id: None,
            git_panel_id: None,
            git_tool,
            bottom_visible: false,
            bottom_ratio: saved_session.bottom_ratio,
            bottom_terminal_tabs: TabManager::new(),
        };

        // Lua初期化エラーがあれば通知する
        if let Some(err) = lua_init_error {
            app.overlay
                .set_status_message_with_level(err, MessageLevel::Error);
        }

        // セッションに保存されていたパネル状態を復元する
        if let Some(ref content) = saved_session.side_content {
            match content.as_str() {
                "files" => app.dispatch_action(Action::ToggleSide(SideContent::FileTree)),
                "git" => app.dispatch_action(Action::ToggleSide(SideContent::Git)),
                _ => {}
            }
        }
        if saved_session.bottom_visible {
            app.dispatch_action(Action::ToggleTerminal);
        }

        app
    }

    /// メインイベントループを実行する。
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        // マウスキャプチャとブラケットペーストモードを有効化
        execute!(io::stdout(), EnableMouseCapture, EnableBracketedPaste)?;

        // kittyキーボードプロトコルを対応端末でのみ有効化する。
        // これによりShift+Enter等の修飾付きキーがCSI uで届き、Enterと区別できる
        // （legacy modeでは両者ともCRで届き判別不能）。
        let kitty_enabled = supports_keyboard_enhancement().unwrap_or(false);
        if kitty_enabled {
            execute!(
                io::stdout(),
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )?;
        }

        loop {
            self.draw(terminal)?;

            // 一時メッセージのカウントダウン
            self.overlay.tick_status_message();

            if self.should_quit {
                break;
            }

            // PTY出力を処理
            self.drain_pty_messages();

            // Luaからのアクションを処理
            self.drain_lua_actions();

            // crossterm イベントをポーリング
            if event::poll(Duration::from_millis(50))? {
                let ev = event::read()?;
                let actions = self.handle_crossterm_event(ev);
                self.dispatch_actions(actions);
            }
        }

        // 有効化していた場合のみkittyキーボードプロトコルを解除する
        if kitty_enabled {
            execute!(io::stdout(), PopKeyboardEnhancementFlags)?;
        }

        execute!(io::stdout(), DisableMouseCapture, DisableBracketedPaste)?;

        // セッション状態を保存する
        session::save(&session::SessionState {
            side_content: self.side_content.map(|c| match c {
                SideContent::FileTree => "files".to_string(),
                SideContent::Git => "git".to_string(),
            }),
            side_ratio: self.side_ratio,
            bottom_visible: self.bottom_visible,
            bottom_ratio: self.bottom_ratio,
        });

        Ok(())
    }

    /// PTYからのメッセージを全て取り出して処理する
    fn drain_pty_messages(&mut self) {
        while let Ok(msg) = self.pty_rx.try_recv() {
            match msg {
                PtyMessage::Output { pane_id, data } => {
                    if let Some(pane) = self.panes.get_mut(&pane_id) {
                        pane.process_output(&data);
                    }
                }
                PtyMessage::Exited { pane_id } => {
                    // パネルスロット方式ではPTY終了時もペインを維持する。
                    // PTY終了フラグを立て、最終出力と終了表示を保持する。
                    if let Some(pane) = self.panes.get_mut(&pane_id) {
                        if let Some(tp) = pane.as_any_mut().downcast_mut::<TerminalPane>() {
                            tp.set_pty_dead();
                        } else if let Some(ap) = pane.as_any_mut().downcast_mut::<AgentPane>() {
                            ap.set_pty_dead();
                        }
                    }
                }
            }
        }
    }

    /// Luaからのアクションを処理する
    fn drain_lua_actions(&mut self) {
        while let Ok(action) = self.lua_action_rx.try_recv() {
            self.dispatch_action(action);
        }
    }

    /// crossterm Eventを処理し、Actionリストを返す
    fn handle_crossterm_event(&mut self, ev: Event) -> Vec<Action> {
        match ev {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            Event::Paste(text) => self.handle_paste(text),
            Event::Resize(_w, _h) => vec![Action::Redraw],
            _ => vec![],
        }
    }

    /// キー入力をモードに応じてルーティングする
    fn handle_key(&mut self, key: KeyEvent) -> Vec<Action> {
        // オーバーレイの優先順位: help > command_palette > file_prompt > file_info > file_preview > rename_input
        if self.overlay.help_visible {
            return self.handle_help_key(key);
        }
        if self.overlay.command_palette.is_some() {
            return self.handle_command_palette_key(key);
        }
        if self.overlay.file_prompt.is_some() {
            return self.handle_file_prompt_key(key);
        }
        if self.overlay.file_info.is_some() {
            return self.handle_file_info_key(key);
        }
        if self.overlay.file_preview.is_some() {
            return self.handle_preview_key(key);
        }
        // リネームモード中はリネーム入力を優先処理
        if self.overlay.rename_input.is_some() {
            return self.handle_rename_key(key);
        }

        // コピーモード中はモード別ディスパッチより先に処理する
        let in_copy_mode = self
            .panes
            .get(&self.focused)
            .and_then(|p| {
                p.as_any()
                    .downcast_ref::<TerminalPane>()
                    .map(|tp| tp.is_copy_mode())
                    .or_else(|| {
                        p.as_any()
                            .downcast_ref::<AgentPane>()
                            .map(|ap| ap.is_copy_mode())
                    })
            })
            .unwrap_or(false);
        if in_copy_mode {
            return self.handle_copy_mode_key(key);
        }

        // Luaカスタムキーマップのチェック（組み込みキーマップより優先）
        if key.kind == KeyEventKind::Press
            && let Some(actions) = self.try_lua_keymap(&key)
        {
            return actions;
        }

        if self.global_layer {
            self.handle_global_key(key)
        } else {
            self.handle_direct_key(key)
        }
    }

    /// グローバルレイヤーのトグルキー（Vim記法）を返す。
    /// Lua側で変更されていればその値、なければデフォルト値。
    fn layer_toggle_key(&self) -> String {
        self.keymap_registry
            .as_ref()
            .and_then(|r| r.lock().ok().map(|reg| reg.toggle_key().to_string()))
            .unwrap_or_else(|| DEFAULT_TOGGLE_KEY.to_string())
    }

    /// キーイベントがグローバルレイヤーのトグルキーかどうかを判定する
    pub(super) fn is_layer_toggle(&self, key: &KeyEvent) -> bool {
        key_event_to_string(key).is_some_and(|s| s == self.layer_toggle_key())
    }

    /// Luaキーマップレジストリからマッチするキーバインドを探して実行する。
    /// マッチした場合はActionリストを返し、マッチしなければNoneを返す。
    fn try_lua_keymap(&self, key: &KeyEvent) -> Option<Vec<Action>> {
        let registry = self.keymap_registry.as_ref()?;
        let key_str = key_event_to_string(key)?;
        let context = if self.global_layer {
            KeymapContext::Global
        } else {
            KeymapContext::Direct
        };
        let reg = registry.lock().ok()?;
        let entry = reg.get(context, &key_str)?;

        // コールバック実行（Luaコールバック内でkrk.pane.* 等が呼ばれ、
        // action_tx経由でActionが送信される）
        if let Some(ref lua) = self.lua_runtime {
            let _ = lua.exec_callback(&entry.callback);
        }
        // コールバック内のAction送信はdrain_lua_actionsで処理されるため、
        // ここでは空のVecを返してキーイベントの消費を示す
        Some(vec![])
    }

    /// Actionリストを順次ディスパッチする
    fn dispatch_actions(&mut self, actions: Vec<Action>) {
        for action in actions {
            self.dispatch_action(action);
        }
    }

    /// 単一のActionをディスパッチする
    fn dispatch_action(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.should_quit = true;
            }
            Action::PtyWrite { pane_id, data } => {
                if let Some(pane) = self.panes.get_mut(&pane_id) {
                    pane.write_to_pty(&data);
                }
            }
            Action::FocusNext => {
                let ids = self.layout.pane_ids();
                if let Some(pos) = ids.iter().position(|id| *id == self.focused) {
                    self.focused = ids[(pos + 1) % ids.len()];
                }
            }
            Action::FocusPrev => {
                let ids = self.layout.pane_ids();
                if let Some(pos) = ids.iter().position(|id| *id == self.focused) {
                    self.focused = ids[if pos == 0 { ids.len() - 1 } else { pos - 1 }];
                }
            }
            Action::FocusDirection(direction) => {
                if let Some(neighbor) =
                    self.layout
                        .find_neighbor(self.focused, direction, self.last_area)
                {
                    self.focused = neighbor;
                }
            }
            Action::FocusPane(pane_id) => {
                let visible = self.layout.pane_ids();
                if visible.contains(&pane_id) {
                    self.focused = pane_id;
                }
            }
            Action::ResizePane { direction, amount } => {
                let delta = amount as f32 / 100.0;
                self.resize_panel(direction, delta);
            }
            Action::ToggleSide(content) => {
                self.toggle_side(content);
            }
            Action::ToggleFileTree => {
                self.toggle_side(SideContent::FileTree);
            }
            Action::ToggleTerminal => {
                self.toggle_bottom_terminal();
            }
            Action::ToggleGitPanel => {
                self.toggle_side(SideContent::Git);
            }
            Action::NewTab => {
                self.new_tab();
            }
            Action::CloseTab => {
                self.close_tab();
            }
            Action::NextTab => {
                self.next_tab();
            }
            Action::PrevTab => {
                self.prev_tab();
            }
            Action::SelectTab(index) => {
                self.select_tab(index);
            }
            Action::RenameTab(name) => {
                self.main_tabs.rename_active(name);
            }
            Action::NewTerminalTab => {
                self.new_terminal_tab();
            }
            Action::CloseTerminalTab => {
                self.close_terminal_tab();
            }
            Action::NextTerminalTab => {
                self.next_terminal_tab();
            }
            Action::PrevTerminalTab => {
                self.prev_terminal_tab();
            }
            Action::SelectTerminalTab(index) => {
                self.select_terminal_tab(index);
            }
            Action::RenameTerminalTab(name) => {
                self.bottom_terminal_tabs.rename_active(name);
            }
            Action::ToggleFilePreview(path) => {
                // 既にプレビュー表示中なら閉じる
                if self
                    .overlay
                    .file_preview
                    .as_ref()
                    .is_some_and(|p| p.path == path)
                {
                    self.overlay.file_preview = None;
                } else {
                    self.overlay.file_preview = Some(FilePreview::load(path));
                }
            }
            Action::OpenFilePrompt(kind) => {
                let input = match &kind {
                    FilePromptKind::Rename { current_name, .. } => current_name.clone(),
                    _ => String::new(),
                };
                self.overlay.file_prompt = Some(FilePrompt { kind, input });
            }
            Action::ShowFileInfo(path) => {
                self.overlay.file_info = Some(FileInfo::load(path));
            }
            Action::CopyToClipboard(text) => {
                self.copy_to_clipboard(&text);
            }
            Action::SendFileToAgent(path) => {
                self.send_file_to_agent(path);
            }
            Action::EnterCopyMode => {
                if let Some(pane) = self.panes.get_mut(&self.focused) {
                    if let Some(tp) = pane.as_any_mut().downcast_mut::<TerminalPane>() {
                        tp.enter_copy_mode();
                    } else if let Some(ap) = pane.as_any_mut().downcast_mut::<AgentPane>() {
                        ap.enter_copy_mode();
                    }
                }
            }
            Action::ExitCopyMode => {
                if let Some(pane) = self.panes.get_mut(&self.focused) {
                    if let Some(tp) = pane.as_any_mut().downcast_mut::<TerminalPane>() {
                        tp.exit_copy_mode();
                    } else if let Some(ap) = pane.as_any_mut().downcast_mut::<AgentPane>() {
                        ap.exit_copy_mode();
                    }
                }
            }
            Action::ShowHelp => {
                self.overlay.help_visible = true;
            }
            Action::Notify(msg) => {
                self.overlay.set_status_message(msg);
            }
            Action::Redraw => {
                // 描画はメインループ先頭の draw() で毎フレーム行われるため、
                // ここでは何もしない。ターミナルリサイズ時等に次フレームでの再描画を保証する。
            }
        }
    }

    /// メインタブのアクティブペインIDを取得する。
    /// main_tabsは常に1つ以上のタブを持つ不変条件があるため、失敗時はパニックする。
    fn main_active_id(&self) -> PaneId {
        self.main_tabs
            .active_id()
            .expect("invariant: main_tabs always has at least one tab")
    }

    /// ペインIDを発行する
    fn alloc_pane_id(&mut self) -> PaneId {
        let id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        id
    }

    /// サイドパネルの表示内容を切り替える。
    /// 同じ内容を指定すると閉じる。別の内容を指定すると入れ替える。
    ///
    /// @param content - 表示するサブパネルの種別
    fn toggle_side(&mut self, content: SideContent) {
        if self.side_content == Some(content) {
            self.side_content = None;
            self.focus_main_if_hidden();
        } else {
            if !self.ensure_side_pane(content) {
                return;
            }
            self.side_content = Some(content);
            if let Some(id) = self.active_side_pane_id() {
                self.focused = id;
            }
        }
        self.rebuild_layout();
    }

    /// ボトムターミナルの表示/非表示を切り替える。
    /// 初回はターミナルペインを生成する。
    fn toggle_bottom_terminal(&mut self) {
        if self.bottom_terminal_tabs.is_empty() {
            let new_id = self.alloc_pane_id();
            let pane = TerminalPane::new(new_id, 80, 24, self.pty_tx.clone());
            self.panes.insert(new_id, Box::new(pane));
            self.bottom_terminal_tabs.add(new_id);
            self.bottom_visible = true;
        } else {
            self.bottom_visible = !self.bottom_visible;
        }

        if self.bottom_visible {
            if let Some(id) = self.bottom_terminal_tabs.active_id() {
                self.focused = id;
            }
        } else {
            self.focus_main_if_hidden();
        }
        self.rebuild_layout();
    }

    /// サイドパネルの遅延初期化。初回アクセス時のみペインを生成する。
    /// Gitツールが見つからない場合はfalseを返す。
    ///
    /// @param content - 初期化するサブパネルの種別
    /// @returns 初期化成功ならtrue
    fn ensure_side_pane(&mut self, content: SideContent) -> bool {
        match content {
            SideContent::FileTree => {
                if self.file_tree_id.is_none() {
                    let new_id = self.alloc_pane_id();
                    let path = std::env::current_dir().unwrap_or_else(|_| "/".into());
                    let pane = FileTreePane::new(new_id, path);
                    self.panes.insert(new_id, Box::new(pane));
                    self.file_tree_id = Some(new_id);
                }
                true
            }
            SideContent::Git => {
                if self.git_panel_id.is_none() {
                    if which::which(&self.git_tool).is_err() {
                        self.overlay.set_status_message_with_level(
                            format!(
                                "{} not found. Install it or set krk.opt.git_tool",
                                self.git_tool
                            ),
                            MessageLevel::Warn,
                        );
                        return false;
                    }
                    let new_id = self.alloc_pane_id();
                    let pane = TerminalPane::with_command(
                        new_id,
                        &self.git_tool,
                        &[],
                        &self.git_tool,
                        80,
                        24,
                        self.pty_tx.clone(),
                    );
                    self.panes.insert(new_id, Box::new(pane));
                    self.git_panel_id = Some(new_id);
                }
                true
            }
        }
    }

    /// 現在のside_contentに対応するPaneIdを返す。
    ///
    /// @returns サイドパネルが表示中ならそのPaneId
    fn active_side_pane_id(&self) -> Option<PaneId> {
        match self.side_content? {
            SideContent::FileTree => self.file_tree_id,
            SideContent::Git => self.git_panel_id,
        }
    }

    /// フォーカス中のペインがボトムターミナルタブかどうか
    fn is_bottom_terminal_focused(&self) -> bool {
        self.bottom_visible && self.bottom_terminal_tabs.contains(&self.focused)
    }

    /// フォーカス中のペインが非表示になった場合、メインペインにフォーカスを戻す
    fn focus_main_if_hidden(&mut self) {
        let visible = self.layout.pane_ids();
        if !visible.contains(&self.focused) {
            self.focused = self.main_active_id();
        }
    }

    /// レイアウトツリーを再構築する。
    /// メイン(+ボトムターミナル) + サイドパネル（表示中なら）の分割を構築する。
    fn rebuild_layout(&mut self) {
        let main_id = self.main_active_id();

        // メイン + ボトムターミナル（水平分割）
        let center = if self.bottom_visible {
            if let Some(bottom_id) = self.bottom_terminal_tabs.active_id() {
                LayoutNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: self.bottom_ratio,
                    first: Box::new(LayoutNode::Leaf(main_id)),
                    second: Box::new(LayoutNode::Leaf(bottom_id)),
                }
            } else {
                LayoutNode::Leaf(main_id)
            }
        } else {
            LayoutNode::Leaf(main_id)
        };

        // 中央 + サイドパネル（垂直分割）
        self.layout = if let Some(side_id) = self.active_side_pane_id() {
            LayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 1.0 - self.side_ratio,
                first: Box::new(center),
                second: Box::new(LayoutNode::Leaf(side_id)),
            }
        } else {
            center
        };

        // フォーカスが可視ペインに含まれることを保証
        let visible_ids = self.layout.pane_ids();
        if !visible_ids.contains(&self.focused) {
            self.focused = self.main_active_id();
        }
    }

    /// パネルの分割比率を変更する。
    fn resize_panel(&mut self, direction: Direction, delta: f32) {
        match direction {
            Direction::Left | Direction::Right => {
                if self.side_content.is_some() {
                    let adjust = if direction == Direction::Left {
                        delta
                    } else {
                        -delta
                    };
                    self.side_ratio = (self.side_ratio + adjust).clamp(0.15, 0.6);
                    self.rebuild_layout();
                }
            }
            Direction::Up | Direction::Down => {
                if self.bottom_visible && self.bottom_terminal_tabs.contains(&self.focused) {
                    let adjust = if direction == Direction::Down {
                        delta
                    } else {
                        -delta
                    };
                    self.bottom_ratio = (self.bottom_ratio + adjust).clamp(0.3, 0.9);
                    self.rebuild_layout();
                }
            }
        }
    }

    /// 新しいエージェントタブを追加し、アクティブに切り替える
    fn new_tab(&mut self) {
        let new_id = self.alloc_pane_id();
        let pane = AgentPane::new(new_id, &self.tab_provider, 80, 24, self.pty_tx.clone());
        self.panes.insert(new_id, Box::new(pane));
        self.main_tabs.add(new_id);
        self.focused = new_id;
        self.rebuild_layout();
    }

    /// アクティブなタブを閉じる（最後の1つは閉じない）
    fn close_tab(&mut self) {
        if self.main_tabs.len() <= 1 {
            return;
        }
        if let Some(removed_id) = self.main_tabs.remove_active() {
            self.panes.remove(&removed_id);
        }
        self.focused = self.main_active_id();
        self.rebuild_layout();
    }

    /// 次のタブに切り替える（循環）
    fn next_tab(&mut self) {
        self.main_tabs.next();
        self.focused = self.main_active_id();
        self.rebuild_layout();
    }

    /// 前のタブに切り替える（循環）
    fn prev_tab(&mut self) {
        self.main_tabs.prev();
        self.focused = self.main_active_id();
        self.rebuild_layout();
    }

    /// インデックス指定でタブを選択する（0始まり）
    fn select_tab(&mut self, index: usize) {
        if index >= self.main_tabs.len() || index == self.main_tabs.active_index() {
            return;
        }
        self.main_tabs.select(index);
        self.focused = self.main_active_id();
        self.rebuild_layout();
    }

    /// 新しいターミナルタブを追加し、アクティブに切り替える
    fn new_terminal_tab(&mut self) {
        let new_id = self.alloc_pane_id();
        let pane = TerminalPane::new(new_id, 80, 24, self.pty_tx.clone());
        self.panes.insert(new_id, Box::new(pane));
        self.bottom_terminal_tabs.add(new_id);
        self.focused = new_id;
        self.rebuild_layout();
    }

    /// アクティブなターミナルタブを閉じる。
    /// 最後のタブを閉じた場合はサイドパネルを閉じる。
    fn close_terminal_tab(&mut self) {
        if self.bottom_terminal_tabs.is_empty() {
            return;
        }
        if let Some(removed_id) = self.bottom_terminal_tabs.remove_active() {
            self.panes.remove(&removed_id);
        }

        if self.bottom_terminal_tabs.is_empty() {
            self.bottom_visible = false;
            self.focused = self.main_active_id();
        } else {
            self.focused = self
                .bottom_terminal_tabs
                .active_id()
                .expect("invariant: bottom_terminal_tabs is non-empty after failed is_empty check");
        }
        self.rebuild_layout();
    }

    /// 次のターミナルタブに切り替える（循環）
    fn next_terminal_tab(&mut self) {
        self.bottom_terminal_tabs.next();
        if let Some(id) = self.bottom_terminal_tabs.active_id() {
            self.focused = id;
            self.rebuild_layout();
        }
    }

    /// 前のターミナルタブに切り替える（循環）
    fn prev_terminal_tab(&mut self) {
        self.bottom_terminal_tabs.prev();
        if let Some(id) = self.bottom_terminal_tabs.active_id() {
            self.focused = id;
            self.rebuild_layout();
        }
    }

    /// インデックス指定でターミナルタブを選択する（0始まり）
    fn select_terminal_tab(&mut self, index: usize) {
        if index >= self.bottom_terminal_tabs.len()
            || index == self.bottom_terminal_tabs.active_index()
        {
            return;
        }
        self.bottom_terminal_tabs.select(index);
        if let Some(id) = self.bottom_terminal_tabs.active_id() {
            self.focused = id;
            self.rebuild_layout();
        }
    }
}

/// XDG設定ディレクトリのパスを返す（~/.config/krk/）
fn dirs_config_path() -> PathBuf {
    dirs_home().join(".config").join("krk")
}

/// ホームディレクトリのパスを返す
fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

/// KeyEventをPTYに送信するバイト列に変換する
fn key_to_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                let byte = (c.to_ascii_lowercase() as u8)
                    .wrapping_sub(b'a')
                    .wrapping_add(1);
                Some(vec![byte])
            } else {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                Some(s.as_bytes().to_vec())
            }
        }
        KeyCode::Enter => {
            // Shift+Enter / Alt+Enter は改行挿入。LF(0x0A)を送る。
            // 多くのCLIエージェント（Claude Code等）はCR=送信/確定、LF=改行挿入として扱うため、
            // 送信先がkitty非対応でも改行として解釈される。
            // なお修飾を検知するにはkittyキーボードプロトコルの有効化が前提（run()参照）。
            if key.modifiers.contains(KeyModifiers::SHIFT)
                || key.modifiers.contains(KeyModifiers::ALT)
            {
                Some(vec![b'\n'])
            } else {
                Some(vec![b'\r'])
            }
        }
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        _ => None,
    }
}

/// crossterm の KeyEvent をキーマップ検索用の文字列に変換する。
/// 例: 'q' → "q", Ctrl+a → "<C-a>", Enter → "<CR>"
fn key_event_to_string(key: &KeyEvent) -> Option<String> {
    match key.code {
        // スペースは "<C- >" のような曖昧な表記を避けるため名前付きで表現する
        KeyCode::Char(' ') => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                Some("<C-Space>".to_string())
            } else {
                Some("<Space>".to_string())
            }
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                Some(format!("<C-{}>", c.to_ascii_lowercase()))
            } else if key.modifiers.contains(KeyModifiers::ALT) {
                Some(format!("<A-{c}>"))
            } else {
                Some(c.to_string())
            }
        }
        KeyCode::Enter => Some("<CR>".to_string()),
        KeyCode::Esc => Some("<Esc>".to_string()),
        KeyCode::Tab => Some("<Tab>".to_string()),
        KeyCode::BackTab => Some("<S-Tab>".to_string()),
        KeyCode::Backspace => Some("<BS>".to_string()),
        KeyCode::Up => Some("<Up>".to_string()),
        KeyCode::Down => Some("<Down>".to_string()),
        KeyCode::Left => Some("<Left>".to_string()),
        KeyCode::Right => Some("<Right>".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
