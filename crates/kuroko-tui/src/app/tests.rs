//! App構造体の統合テスト。
//! 実際のPTYプロセスを生成するApp::new()を使い、状態遷移を公開インターフェース経由で検証する。

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use kuroko_core::{Action, PaneId, PaneType, SideContent};

use super::App;
use super::overlay::CommandPalette;
use super::overlay::MessageLevel;

// ---------------------------------------------------------------------------
// ヘルパー: KeyEvent生成
// ---------------------------------------------------------------------------

/// 指定キーコードのPress KeyEventを生成する
///
/// @param code - 生成するキーコード
/// @returns 修飾キーなしのPress KeyEvent
fn press_key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

/// Escキーイベントを生成する
fn esc_key() -> KeyEvent {
    press_key(KeyCode::Esc)
}

/// 'i' キーイベントを生成する
fn i_key() -> KeyEvent {
    press_key(KeyCode::Char('i'))
}

/// ':' キーイベントを生成する
fn colon_key() -> KeyEvent {
    press_key(KeyCode::Char(':'))
}

/// グローバルモードのトグルキー（Ctrl+g）イベントを生成する
fn toggle_key() -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char('g'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

// ===========================================================================
// 1. グローバルモードの出入り
// ===========================================================================

#[test]
fn starts_in_direct_state() {
    let app = App::new();
    assert!(!app.global_mode);
}

#[test]
fn toggle_key_enters_global_mode() {
    let mut app = App::new();
    let actions = app.handle_key(toggle_key());
    app.dispatch_actions(actions);
    assert!(app.global_mode);
}

#[test]
fn toggle_key_exits_global_mode() {
    let mut app = App::new();
    app.global_mode = true;
    let actions = app.handle_key(toggle_key());
    app.dispatch_actions(actions);
    assert!(!app.global_mode);
}

#[test]
fn esc_exits_global_mode() {
    let mut app = App::new();
    app.global_mode = true;
    let actions = app.handle_global_key(esc_key());
    app.dispatch_actions(actions);
    assert!(!app.global_mode);
}

#[test]
fn i_does_not_exit_global_mode() {
    // i は解除キーから除外済み。グローバルモード中の i は無操作で状態を維持する
    let mut app = App::new();
    app.global_mode = true;
    let actions = app.handle_global_key(i_key());
    assert!(actions.is_empty());
    app.dispatch_actions(actions);
    assert!(app.global_mode);
}

#[test]
fn esc_flows_to_pty_in_direct_state() {
    let mut app = App::new();
    // 直通状態のEscはグローバルモード操作ではなくPTYへ転送される（vim/agent中断のため）
    let actions = app.handle_direct_key(esc_key());
    assert!(!app.global_mode);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::PtyWrite { data, .. } if data == &vec![0x1b_u8]))
    );
}

#[test]
fn q_flows_to_pane_in_direct_state() {
    let mut app = App::new();
    // 直通状態の q は終了ではなくペインへ転送される
    let actions = app.handle_direct_key(press_key(KeyCode::Char('q')));
    app.dispatch_actions(actions);
    assert!(!app.should_quit);
}

// ===========================================================================
// 2. サイドパネル
// ===========================================================================

#[test]
fn toggle_file_tree_creates_pane_on_first_call() {
    let mut app = App::new();
    assert!(app.file_tree_id.is_none());
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    assert!(app.file_tree_id.is_some());
    assert_eq!(app.side_content, Some(SideContent::FileTree));
}

#[test]
fn toggle_file_tree_hides_on_second_call() {
    let mut app = App::new();
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    assert_eq!(app.side_content, Some(SideContent::FileTree));
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    assert_eq!(app.side_content, None);
}

#[test]
fn toggle_file_tree_shows_again_on_third_call() {
    let mut app = App::new();
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    assert_eq!(app.side_content, Some(SideContent::FileTree));
}

#[test]
fn toggle_file_tree_does_not_create_duplicate_pane() {
    let mut app = App::new();
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    let first_id = app.file_tree_id;
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    assert_eq!(app.file_tree_id, first_id);
}

#[test]
fn file_manager_unset_builds_builtin_tree() {
    // file_manager未設定なら内蔵ファイルツリーを生成する
    let mut app = App::new();
    app.file_manager = None;
    let pane = app.build_file_tree_pane(PaneId(99));
    assert_eq!(pane.pane_type(), PaneType::FileTree);
}

#[test]
fn file_manager_existing_command_builds_terminal() {
    // 存在する外部コマンド指定時はPTYで起動するターミナルペインになる
    let mut app = App::new();
    app.file_manager = Some("sh".to_string());
    let pane = app.build_file_tree_pane(PaneId(99));
    assert_eq!(pane.pane_type(), PaneType::Terminal);
}

#[test]
fn file_manager_missing_command_warns_and_falls_back() {
    // 存在しないコマンド指定時は警告を出して内蔵ツリーにフォールバックする
    let mut app = App::new();
    app.file_manager = Some("krk-no-such-file-manager".to_string());
    let pane = app.build_file_tree_pane(PaneId(99));
    assert_eq!(pane.pane_type(), PaneType::FileTree);
    let msg = app.overlay.status_message.expect("warn message expected");
    assert_eq!(msg.level, MessageLevel::Warn);
}

#[test]
fn toggle_terminal_creates_and_shows() {
    let mut app = App::new();
    assert!(!app.bottom_visible);
    app.dispatch_action(Action::ToggleTerminal);
    assert!(app.bottom_visible);
    assert!(!app.bottom_terminal_tabs.is_empty());
}

#[test]
fn toggle_terminal_hides_on_second_call() {
    let mut app = App::new();
    app.dispatch_action(Action::ToggleTerminal);
    assert!(app.bottom_visible);
    app.dispatch_action(Action::ToggleTerminal);
    assert!(!app.bottom_visible);
}

#[test]
fn side_panel_exclusive_display() {
    let mut app = App::new();
    // ファイルツリーを表示
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    assert_eq!(app.side_content, Some(SideContent::FileTree));

    // Gitに切り替え（ファイルツリーは非表示になる）
    app.dispatch_action(Action::ToggleSide(SideContent::Git));
    if app.side_content == Some(SideContent::Git) {
        // lazygitインストール済み環境
        app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
        assert_eq!(app.side_content, Some(SideContent::FileTree));
    }
}

#[test]
fn side_and_bottom_are_independent() {
    let mut app = App::new();
    // サイドパネルとボトムターミナルは独立して開閉
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    app.dispatch_action(Action::ToggleTerminal);
    assert_eq!(app.side_content, Some(SideContent::FileTree));
    assert!(app.bottom_visible);
    // レイアウトはメイン + ターミナル + サイドの3ペイン
    assert_eq!(app.layout.pane_ids().len(), 3);
}

#[test]
fn side_panel_close_returns_to_single_leaf() {
    let mut app = App::new();
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    assert_eq!(app.layout.pane_ids().len(), 2);
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    assert_eq!(app.layout.pane_ids().len(), 1);
}

#[test]
fn legacy_toggle_actions_work() {
    let mut app = App::new();
    app.dispatch_action(Action::ToggleFileTree);
    assert_eq!(app.side_content, Some(SideContent::FileTree));
    app.dispatch_action(Action::ToggleFileTree);
    assert_eq!(app.side_content, None);
    app.dispatch_action(Action::ToggleTerminal);
    assert!(app.bottom_visible);
    app.dispatch_action(Action::ToggleTerminal);
    assert!(!app.bottom_visible);
}

// ===========================================================================
// 3. タブライフサイクル
// ===========================================================================

#[test]
fn initial_state_has_one_main_tab() {
    let app = App::new();
    assert_eq!(app.main_tabs.len(), 1);
}

#[test]
fn new_tab_creates_agent_pane() {
    let mut app = App::new();
    let initial_count = app.main_tabs.len();
    app.dispatch_action(Action::NewTab);
    assert_eq!(app.main_tabs.len(), initial_count + 1);
}

#[test]
fn new_tab_updates_focus_to_new_pane() {
    let mut app = App::new();
    let old_focused = app.focused;
    app.dispatch_action(Action::NewTab);
    assert_ne!(app.focused, old_focused);
    assert_eq!(app.focused, app.main_tabs.active_id().unwrap());
}

#[test]
fn close_tab_keeps_at_least_one() {
    let mut app = App::new();
    app.dispatch_action(Action::CloseTab);
    assert!(!app.main_tabs.is_empty());
}

#[test]
fn close_tab_removes_pane_from_map() {
    let mut app = App::new();
    app.dispatch_action(Action::NewTab);
    let pane_count_before = app.panes.len();
    app.dispatch_action(Action::CloseTab);
    assert_eq!(app.panes.len(), pane_count_before - 1);
}

#[test]
fn next_tab_cycles_through_tabs() {
    let mut app = App::new();
    app.dispatch_action(Action::NewTab);
    app.dispatch_action(Action::NewTab);
    assert_eq!(app.main_tabs.active_index(), 2);

    app.dispatch_action(Action::NextTab);
    assert_eq!(app.main_tabs.active_index(), 0);
}

#[test]
fn prev_tab_cycles_through_tabs() {
    let mut app = App::new();
    app.dispatch_action(Action::NewTab);
    app.dispatch_action(Action::SelectTab(0));
    assert_eq!(app.main_tabs.active_index(), 0);

    app.dispatch_action(Action::PrevTab);
    assert_eq!(app.main_tabs.active_index(), 1);
}

#[test]
fn select_tab_by_index() {
    let mut app = App::new();
    app.dispatch_action(Action::NewTab);
    app.dispatch_action(Action::NewTab);
    app.dispatch_action(Action::SelectTab(0));
    assert_eq!(app.main_tabs.active_index(), 0);
    app.dispatch_action(Action::SelectTab(1));
    assert_eq!(app.main_tabs.active_index(), 1);
}

#[test]
fn select_tab_out_of_range_is_noop() {
    let mut app = App::new();
    app.dispatch_action(Action::SelectTab(5));
    assert_eq!(app.main_tabs.active_index(), 0);
}

// ===========================================================================
// 4. コマンドパレット
// ===========================================================================

#[test]
fn colon_opens_command_palette_in_global_mode() {
    let mut app = App::new();
    app.global_mode = true;
    let actions = app.handle_global_key(colon_key());
    app.dispatch_actions(actions);
    assert!(app.overlay.command_palette.is_some());
}

#[test]
fn esc_closes_command_palette() {
    let mut app = App::new();
    app.overlay.command_palette = Some(CommandPalette::new());
    let actions = app.handle_command_palette_key(esc_key());
    app.dispatch_actions(actions);
    assert!(app.overlay.command_palette.is_none());
}

#[test]
fn show_help_action_opens_and_esc_closes() {
    let mut app = App::new();
    assert!(!app.overlay.help_visible);

    app.dispatch_actions(vec![Action::ShowHelp]);
    assert!(app.overlay.help_visible);

    // ヘルプ表示中はhandle_keyがヘルプ処理に優先ルーティングされる
    let actions = app.handle_key(esc_key());
    app.dispatch_actions(actions);
    assert!(!app.overlay.help_visible);
}

#[test]
fn filetree_receives_keys_directly_in_direct_state() {
    let mut app = App::new();
    app.dispatch_actions(vec![Action::ToggleFileTree]);
    let filetree_id = app.focused;

    // j はフォーカス移動ではなくペイン内カーソル移動として処理される
    let actions = app.handle_direct_key(press_key(KeyCode::Char('j')));
    assert!(
        actions
            .iter()
            .all(|a| !matches!(a, Action::FocusDirection(_)))
    );
    app.dispatch_actions(actions);
    assert_eq!(app.focused, filetree_id);
}

#[test]
fn global_mode_moves_focus_from_filetree() {
    let mut app = App::new();
    app.dispatch_actions(vec![Action::ToggleFileTree]);
    app.global_mode = true;

    // グローバルモード中の h はペイン種別に関わらずフォーカス移動になる
    let actions = app.handle_global_key(press_key(KeyCode::Char('h')));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::FocusDirection(_)))
    );
}

#[test]
fn global_mode_resizes_with_filetree_focused() {
    let mut app = App::new();
    app.dispatch_actions(vec![Action::ToggleFileTree]);
    app.global_mode = true;

    // グローバルモード中の H はfilerフォーカス中でもリサイズになる（旧Normalモードでは不可だった）
    let actions = app.handle_global_key(press_key(KeyCode::Char('H')));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ResizePane { .. }))
    );
}

#[test]
fn panel_toggle_works_in_global_mode_from_filetree() {
    let mut app = App::new();
    app.dispatch_actions(vec![Action::ToggleFileTree]);
    app.global_mode = true;

    // グローバルモード中の f はファイルツリーに渡らずパネルトグルとして処理される
    let actions = app.handle_global_key(press_key(KeyCode::Char('f')));
    assert!(actions.iter().any(|a| matches!(a, Action::ToggleFileTree)));
}

#[test]
fn f_flows_to_filetree_in_direct_state() {
    let mut app = App::new();
    app.dispatch_actions(vec![Action::ToggleFileTree]);

    // 直通状態の f はグローバル操作ではなくペインへ渡る
    let actions = app.handle_direct_key(press_key(KeyCode::Char('f')));
    assert!(actions.iter().all(|a| !matches!(a, Action::ToggleFileTree)));
}

#[test]
fn q_closes_help() {
    let mut app = App::new();
    app.overlay.help_visible = true;
    let actions = app.handle_key(press_key(KeyCode::Char('q')));
    app.dispatch_actions(actions);
    assert!(!app.overlay.help_visible);
}

#[test]
fn command_palette_blocks_global_key_handling() {
    let mut app = App::new();
    app.global_mode = true;
    app.overlay.command_palette = Some(CommandPalette::new());
    let actions = app.handle_key(press_key(KeyCode::Char('q')));
    app.dispatch_actions(actions);
    assert!(!app.should_quit);
}

// ===========================================================================
// 5. 終了アクション
// ===========================================================================

#[test]
fn quit_action_sets_should_quit() {
    let mut app = App::new();
    app.dispatch_action(Action::Quit);
    assert!(app.should_quit);
}

#[test]
fn q_key_in_global_mode_quits() {
    let mut app = App::new();
    app.global_mode = true;
    let actions = app.handle_global_key(press_key(KeyCode::Char('q')));
    app.dispatch_actions(actions);
    assert!(app.should_quit);
}

// ===========================================================================
// 6. 通知アクション
// ===========================================================================

#[test]
fn notify_sets_status_message() {
    let mut app = App::new();
    app.dispatch_action(Action::Notify("test message".to_string()));
    assert!(app.overlay.status_message.is_some());
    assert_eq!(
        app.overlay.status_message.as_ref().unwrap().text,
        "test message"
    );
}

#[test]
fn notify_overwrites_previous_message() {
    let mut app = App::new();
    app.dispatch_action(Action::Notify("first".to_string()));
    app.dispatch_action(Action::Notify("second".to_string()));
    assert_eq!(app.overlay.status_message.as_ref().unwrap().text, "second");
}

// ===========================================================================
// 7. フォーカス操作
// ===========================================================================

#[test]
fn focus_next_cycles_through_visible_panes() {
    let mut app = App::new();
    // サイドパネルを表示して2ペインにする
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    let first = app.focused;
    app.dispatch_action(Action::FocusNext);
    let second = app.focused;
    // 2ペインなので2回nextで元に戻る
    app.dispatch_action(Action::FocusNext);
    assert_eq!(app.focused, first);
    assert_ne!(first, second);
}

#[test]
fn focus_prev_cycles_through_visible_panes() {
    let mut app = App::new();
    app.dispatch_action(Action::ToggleSide(SideContent::FileTree));
    let first = app.focused;
    app.dispatch_action(Action::FocusPrev);
    let second = app.focused;
    app.dispatch_action(Action::FocusPrev);
    assert_eq!(app.focused, first);
    assert_ne!(first, second);
}

// ===========================================================================
// 8. ターミナルタブ操作
// ===========================================================================

#[test]
fn new_terminal_tab_adds_to_bottom_tabs() {
    let mut app = App::new();
    app.dispatch_action(Action::ToggleTerminal);
    assert_eq!(app.bottom_terminal_tabs.len(), 1);
    app.dispatch_action(Action::NewTerminalTab);
    assert_eq!(app.bottom_terminal_tabs.len(), 2);
}

#[test]
fn close_all_terminal_tabs_closes_bottom_panel() {
    let mut app = App::new();
    app.dispatch_action(Action::ToggleTerminal);
    assert!(app.bottom_visible);
    app.dispatch_action(Action::CloseTerminalTab);
    assert!(app.bottom_terminal_tabs.is_empty());
    assert!(!app.bottom_visible);
}

// ===========================================================================
// 9. コピーモードとグローバルモードの連携
// ===========================================================================

#[test]
fn enter_in_global_mode_starts_copy_mode_and_exits_layer() {
    let mut app = App::new();
    app.global_mode = true;
    // メインペイン（エージェント）フォーカス中のEnterはコピーモード開始
    let actions = app.handle_global_key(press_key(KeyCode::Enter));
    assert!(actions.iter().any(|a| matches!(a, Action::EnterCopyMode)));
    // コピーモードはペイン内部状態のためグローバルモードは抜ける
    assert!(!app.global_mode);
}

#[test]
fn enter_in_copy_mode_exits_copy_mode_to_direct() {
    let mut app = App::new();
    // コピーモード開始（グローバルモード経由で開始し、グローバルモードは抜ける）
    app.global_mode = true;
    app.handle_global_key(press_key(KeyCode::Enter))
        .into_iter()
        .for_each(|a| app.dispatch_action(a));
    assert!(!app.global_mode);

    // コピーモード中のEnterは終了アクションを返す（グローバルへは戻らない）
    let actions = app.handle_copy_mode_key(press_key(KeyCode::Enter));
    assert!(actions.iter().any(|a| matches!(a, Action::ExitCopyMode)));
    actions.into_iter().for_each(|a| app.dispatch_action(a));
    assert!(!app.global_mode);
}

// ===========================================================================
// 10. Redrawアクション
// ===========================================================================

#[test]
fn redraw_action_is_noop() {
    let mut app = App::new();
    let layer_before = app.global_mode;
    let focused_before = app.focused;
    app.dispatch_action(Action::Redraw);
    assert_eq!(app.global_mode, layer_before);
    assert_eq!(app.focused, focused_before);
}

// ===========================================================================
// 11. key_to_bytes: PTYへ送るバイト列変換
// ===========================================================================

#[test]
fn tab_sends_horizontal_tab() {
    assert_eq!(
        super::key_to_bytes(&press_key(KeyCode::Tab)),
        Some(vec![b'\t'])
    );
}

#[test]
fn backtab_sends_csi_z() {
    // Shift+Tab（crosstermではBackTab）は逆タブCSI Zを送る。
    // これがないとエージェント（Claude Code等）のモード切替が効かない。
    assert_eq!(
        super::key_to_bytes(&press_key(KeyCode::BackTab)),
        Some(b"\x1b[Z".to_vec())
    );
}

// ---------------------------------------------------------------------------
// エディタダイアログ
// ---------------------------------------------------------------------------

#[test]
fn parse_command_program_only() {
    // 引数なしのコマンドはプログラム名のみ、引数は空になる
    let (program, args) = super::parse_command("vim");
    assert_eq!(program, "vim");
    assert!(args.is_empty());
}

#[test]
fn parse_command_with_args() {
    // 空白区切りで先頭がプログラム名、残りが引数になる
    let (program, args) = super::parse_command("nvim -u NONE");
    assert_eq!(program, "nvim");
    assert_eq!(args, vec!["-u".to_string(), "NONE".to_string()]);
}

#[test]
fn parse_command_empty_yields_empty_program() {
    // 空文字列はプログラム名が空になる（呼び出し側が非空を保証する前提）
    let (program, args) = super::parse_command("");
    assert!(program.is_empty());
    assert!(args.is_empty());
}

#[test]
fn editor_dialog_rect_is_centered_and_within_area() {
    use ratatui::layout::Rect;
    let area = Rect::new(0, 0, 100, 40);
    let dialog = super::App::editor_dialog_rect(area);
    // 画面の約9割サイズで、画面内に収まる
    assert_eq!(dialog.width, 90);
    assert_eq!(dialog.height, 36);
    assert!(dialog.right() <= area.right());
    assert!(dialog.bottom() <= area.bottom());
    // 中央配置（左右・上下の余白が均等）
    assert_eq!(dialog.x, (area.width - dialog.width) / 2);
    assert_eq!(dialog.y, (area.height - dialog.height) / 2);
}
