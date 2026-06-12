//! App構造体の統合テスト。
//! 実際のPTYプロセスを生成するApp::new()を使い、状態遷移を公開インターフェース経由で検証する。

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use kuroko_core::{Action, Mode, SideContent};

use super::App;
use super::overlay::CommandPalette;

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

// ===========================================================================
// 1. モード遷移
// ===========================================================================

#[test]
fn mode_starts_as_insert() {
    let app = App::new();
    assert_eq!(app.mode, Mode::Insert);
}

#[test]
fn esc_switches_to_normal_mode() {
    let mut app = App::new();
    let actions = app.handle_insert_key(esc_key());
    app.dispatch_actions(actions);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn i_switches_to_insert_mode() {
    let mut app = App::new();
    app.mode = Mode::Normal;
    let actions = app.handle_normal_key(i_key());
    app.dispatch_actions(actions);
    assert_eq!(app.mode, Mode::Insert);
}

#[test]
fn mode_round_trip_insert_normal_insert() {
    let mut app = App::new();
    assert_eq!(app.mode, Mode::Insert);

    // Insert -> Normal
    let actions = app.handle_insert_key(esc_key());
    app.dispatch_actions(actions);
    assert_eq!(app.mode, Mode::Normal);

    // Normal -> Insert
    let actions = app.handle_normal_key(i_key());
    app.dispatch_actions(actions);
    assert_eq!(app.mode, Mode::Insert);
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
fn colon_opens_command_palette() {
    let mut app = App::new();
    app.mode = Mode::Normal;
    let actions = app.handle_normal_key(colon_key());
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
fn filetree_receives_keys_directly_in_normal_mode() {
    let mut app = App::new();
    app.dispatch_actions(vec![Action::ToggleFileTree]);
    let filetree_id = app.focused;

    // j はフォーカス移動ではなくペイン内カーソル移動として処理される
    let actions = app.handle_normal_key(press_key(KeyCode::Char('j')));
    assert!(
        actions
            .iter()
            .all(|a| !matches!(a, Action::FocusDirection(_)))
    );
    app.dispatch_actions(actions);
    assert_eq!(app.focused, filetree_id);
}

#[test]
fn ctrl_hjkl_moves_focus_from_filetree() {
    let mut app = App::new();
    app.dispatch_actions(vec![Action::ToggleFileTree]);

    let key = KeyEvent {
        code: KeyCode::Char('h'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    let actions = app.handle_normal_key(key);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::FocusDirection(_)))
    );
}

#[test]
fn global_toggle_keys_work_from_filetree() {
    let mut app = App::new();
    app.dispatch_actions(vec![Action::ToggleFileTree]);

    // f はファイルツリーに渡らずグローバルのパネルトグルとして処理される
    let actions = app.handle_normal_key(press_key(KeyCode::Char('f')));
    assert!(actions.iter().any(|a| matches!(a, Action::ToggleFileTree)));
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
fn command_palette_blocks_normal_key_handling() {
    let mut app = App::new();
    app.mode = Mode::Normal;
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
fn q_key_in_normal_mode_quits() {
    let mut app = App::new();
    app.mode = Mode::Normal;
    let actions = app.handle_normal_key(press_key(KeyCode::Char('q')));
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
// 9. set_mode アクション
// ===========================================================================

#[test]
fn set_mode_action_changes_mode() {
    let mut app = App::new();
    assert_eq!(app.mode, Mode::Insert);
    app.dispatch_action(Action::SetMode(Mode::Normal));
    assert_eq!(app.mode, Mode::Normal);
    app.dispatch_action(Action::SetMode(Mode::Insert));
    assert_eq!(app.mode, Mode::Insert);
}

// ===========================================================================
// 10. Redrawアクション
// ===========================================================================

#[test]
fn redraw_action_is_noop() {
    let mut app = App::new();
    let mode_before = app.mode;
    let focused_before = app.focused;
    app.dispatch_action(Action::Redraw);
    assert_eq!(app.mode, mode_before);
    assert_eq!(app.focused, focused_before);
}
