//! App構造体の統合テスト。
//! 実際のPTYプロセスを生成するApp::new()を使い、状態遷移を公開インターフェース経由で検証する。

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use std::time::Duration;

use kuroko_core::{Action, AppEvent, PaneId, PaneType, SideContent};
use kuroko_terminal::pty_handle::PtyMessage;

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
fn close_last_tab_replaces_with_new_session() {
    let mut app = App::new();
    let old_id = app.main_tabs.active_id().unwrap();
    let pane_count_before = app.panes.len();
    app.dispatch_action(Action::CloseTab);
    let new_id = app.main_tabs.active_id().unwrap();
    assert_eq!(app.main_tabs.len(), 1);
    assert_ne!(new_id, old_id);
    assert!(!app.panes.contains_key(&old_id));
    assert!(app.panes.contains_key(&new_id));
    assert_eq!(app.panes.len(), pane_count_before);
    assert_eq!(app.focused, new_id);
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
fn exited_agent_is_replaced_with_new_session() {
    let mut app = App::new();
    app.respawn_min_uptime = Duration::ZERO;
    let old_id = app.main_tabs.active_id().unwrap();
    app.pty_tx
        .send(PtyMessage::Exited { pane_id: old_id })
        .unwrap();
    app.drain_pty_messages();
    let new_id = app.main_tabs.active_id().unwrap();
    assert_eq!(app.main_tabs.len(), 1);
    assert_ne!(new_id, old_id);
    assert!(!app.panes.contains_key(&old_id));
    assert!(app.panes.contains_key(&new_id));
    assert_eq!(app.focused, new_id);
}

/// Ctrl+C イベントを生成する
fn ctrl_c_key() -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn double_ctrl_c_then_agent_exit_quits_app() {
    let mut app = App::new();
    app.respawn_min_uptime = Duration::ZERO;
    let id = app.main_tabs.active_id().unwrap();
    // 2回とも PTY へ転送される（キーを奪わない）
    for _ in 0..2 {
        let actions = app.handle_direct_key(ctrl_c_key());
        assert!(matches!(actions.as_slice(), [Action::PtyWrite { .. }]));
    }
    app.pty_tx.send(PtyMessage::Exited { pane_id: id }).unwrap();
    app.drain_pty_messages();
    assert!(app.should_quit);
}

#[test]
fn single_ctrl_c_then_agent_exit_does_not_quit() {
    let mut app = App::new();
    app.respawn_min_uptime = Duration::ZERO;
    let id = app.main_tabs.active_id().unwrap();
    app.handle_direct_key(ctrl_c_key());
    app.pty_tx.send(PtyMessage::Exited { pane_id: id }).unwrap();
    app.drain_pty_messages();
    assert!(!app.should_quit);
    assert_ne!(app.main_tabs.active_id(), Some(id));
}

#[test]
fn quickly_exited_agent_is_kept_to_avoid_respawn_loop() {
    let mut app = App::new();
    app.respawn_min_uptime = Duration::from_secs(3600);
    let old_id = app.main_tabs.active_id().unwrap();
    app.pty_tx
        .send(PtyMessage::Exited { pane_id: old_id })
        .unwrap();
    app.drain_pty_messages();
    assert_eq!(app.main_tabs.active_id(), Some(old_id));
    assert!(app.panes.contains_key(&old_id));
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

#[test]
fn choose_editor_prefers_explicit_setting() {
    let cmd = super::choose_editor([Some("hx".to_string()), Some("emacs".to_string())], |_| {
        true
    });
    assert_eq!(cmd, "hx");
}

#[test]
fn choose_editor_skips_empty_values() {
    let cmd = super::choose_editor(
        [None, Some("  ".to_string()), Some("nano".to_string())],
        |_| true,
    );
    assert_eq!(cmd, "nano");
}

#[test]
fn choose_editor_falls_back_to_nvim_when_available() {
    let cmd = super::choose_editor([None, None], |p| p == "nvim");
    assert_eq!(cmd, "nvim");
}

#[test]
fn choose_editor_falls_back_to_vim_without_nvim() {
    let cmd = super::choose_editor([None, None], |_| false);
    assert_eq!(cmd, "vim");
}

// ---------------------------------------------------------------------------
// セッション一覧サイドバー
// ---------------------------------------------------------------------------

#[test]
fn toggle_sessions_shows_sidebar_on_left_and_focuses_it() {
    let mut app = App::new();
    app.dispatch_action(Action::ToggleSessions);
    let sessions_id = app.sessions_id.expect("sessions pane is created");
    assert_eq!(app.focused, sessions_id);
    assert_eq!(app.layout.pane_ids().first(), Some(&sessions_id));
    assert_eq!(
        app.panes.get(&sessions_id).map(|p| p.pane_type()),
        Some(PaneType::Sessions)
    );

    app.dispatch_action(Action::ToggleSessions);
    assert!(!app.layout.pane_ids().contains(&sessions_id));
    assert_eq!(app.focused, app.main_active_id());
}

#[test]
fn global_s_toggles_sessions() {
    let mut app = App::new();
    app.global_mode = true;
    let actions = app.handle_global_key(press_key(KeyCode::Char('s')));
    assert!(actions.iter().any(|a| matches!(a, Action::ToggleSessions)));
}

#[test]
fn global_oneshot_keys_return_to_direct() {
    // フォーカス移動・パネルトグル・パレット起動は実行後に直通へ戻る
    for key in [
        KeyCode::Char('h'),
        KeyCode::Char('j'),
        KeyCode::Char('k'),
        KeyCode::Char('l'),
        KeyCode::Tab,
        KeyCode::BackTab,
        KeyCode::Char('f'),
        KeyCode::Char('t'),
        KeyCode::Char('g'),
        KeyCode::Char('s'),
        KeyCode::Char(':'),
        KeyCode::Char('?'),
    ] {
        let mut app = App::new();
        app.global_mode = true;
        app.handle_global_key(press_key(key));
        assert!(!app.global_mode, "{key:?} は直通へ戻るべき");
    }
}

#[test]
fn question_mark_in_global_mode_opens_help() {
    let mut app = App::new();
    app.global_mode = true;
    let actions = app.handle_global_key(press_key(KeyCode::Char('?')));
    assert!(matches!(actions.as_slice(), [Action::ShowHelp]));
    assert!(!app.global_mode);
}

/// ヘルプの見出し（空キーの行）だけを取り出す
fn help_headings(focus: Option<PaneType>) -> Vec<&'static str> {
    super::render::help_entries(focus)
        .into_iter()
        .filter(|(key, _)| key.is_empty())
        .map(|(_, desc)| desc)
        .collect()
}

#[test]
fn help_shows_global_and_focused_area_only() {
    assert_eq!(help_headings(None), vec!["Global mode"]);
    assert_eq!(
        help_headings(Some(PaneType::Agent)),
        vec![
            "Agent / Terminal (direct input)",
            "Copy mode",
            "Global mode"
        ]
    );
    assert_eq!(
        help_headings(Some(PaneType::Terminal)),
        help_headings(Some(PaneType::Agent))
    );
    assert_eq!(
        help_headings(Some(PaneType::FileTree)),
        vec!["File tree (focused, direct input)", "Global mode"]
    );
    assert_eq!(
        help_headings(Some(PaneType::Sessions)),
        vec!["Sessions sidebar (focused, direct input)", "Global mode"]
    );
}

#[test]
fn help_key_column_fits_longest_key_in_every_context() {
    use super::render::{help_entries, help_key_width};
    for focus in [
        None,
        Some(PaneType::Agent),
        Some(PaneType::FileTree),
        Some(PaneType::Sessions),
    ] {
        let entries = help_entries(focus);
        let width = help_key_width(&entries);
        for (key, _) in &entries {
            assert!(key.chars().count() <= width);
        }
    }
    // Copy mode の "Enter / q / Esc"（15文字）が列幅を超えないこと
    assert!(help_key_width(&help_entries(Some(PaneType::Agent))) >= 15);
    // Global mode の "t / f / g / s"（13文字）が列幅に収まること
    assert!(help_key_width(&help_entries(None)) >= 13);
}

#[test]
fn help_hint_shows_toggle_key_in_direct_and_question_mark_in_global() {
    let mut app = App::new();
    assert_eq!(app.help_hint(), " C-g ? help ");
    app.global_mode = true;
    assert_eq!(app.help_hint(), " ? help ");
}

#[test]
fn global_main_tab_oneshot_keys_return_to_direct() {
    for key in [KeyCode::Char('n'), KeyCode::Char('1'), KeyCode::Char('r')] {
        let mut app = App::new();
        assert!(app.main_tabs.contains(&app.focused));
        app.global_mode = true;
        app.handle_global_key(press_key(key));
        assert!(!app.global_mode, "{key:?} は直通へ戻るべき");
    }
}

#[test]
fn global_repeat_keys_stay_in_global_mode() {
    // リサイズ・タブ順送りは連打のため持続する
    for key in [
        KeyCode::Char('H'),
        KeyCode::Char('J'),
        KeyCode::Char('K'),
        KeyCode::Char('L'),
        KeyCode::Char(']'),
        KeyCode::Char('['),
    ] {
        let mut app = App::new();
        app.global_mode = true;
        app.handle_global_key(press_key(key));
        assert!(app.global_mode, "{key:?} は持続すべき");
    }
}

#[test]
fn global_unknown_key_stays_in_global_mode() {
    let mut app = App::new();
    app.global_mode = true;
    app.handle_global_key(press_key(KeyCode::Char('z')));
    assert!(app.global_mode);
}

#[test]
fn new_tabs_get_unique_session_ids() {
    let mut app = App::new();
    app.dispatch_action(Action::NewTab);
    let ids: Vec<String> = app
        .main_tabs
        .tabs()
        .iter()
        .filter_map(|id| {
            app.panes
                .get(id)
                .and_then(|p| p.as_any().downcast_ref::<kuroko_agent::AgentPane>())
                .map(|a| a.session_id().to_string())
        })
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn resume_session_opens_named_tab_then_reuses_it() {
    let mut app = App::new();
    let resume = || Action::ResumeSession {
        id: "11111111-2222-3333-4444-555555555555".to_string(),
        title: "Fix the login bug in the auth middleware".to_string(),
    };
    app.dispatch_action(resume());
    assert_eq!(app.main_tabs.len(), 2);
    assert_eq!(app.main_tabs.active_index(), 1);
    let name = app.main_tabs.names()[1].clone().unwrap();
    assert_eq!(name.chars().count(), 24);
    assert!(name.ends_with('…'));

    // 別タブへ移ってから同じセッションを再開すると、既存タブへ切り替わるだけ
    app.dispatch_action(Action::SelectTab(0));
    app.dispatch_action(resume());
    assert_eq!(app.main_tabs.len(), 2);
    assert_eq!(app.main_tabs.active_index(), 1);
    assert_eq!(app.focused, app.main_active_id());
}

#[test]
fn select_active_tab_moves_focus_to_main() {
    let mut app = App::new();
    app.dispatch_action(Action::ToggleSessions);
    app.dispatch_action(Action::SelectTab(0));
    assert_eq!(app.focused, app.main_active_id());
}

#[test]
fn new_session_from_sidebar_closes_it() {
    let mut app = App::new();
    app.dispatch_action(Action::ToggleSessions);
    let sessions_id = app.sessions_id.expect("sessions pane is created");
    let before = app.main_tabs.len();

    let actions = app
        .panes
        .get_mut(&sessions_id)
        .expect("sessions pane exists")
        .handle_event(&AppEvent::Key(press_key(KeyCode::Char('n'))));
    app.dispatch_actions(actions);

    assert_eq!(app.main_tabs.len(), before + 1);
    assert!(!app.sessions_visible);
    assert!(!app.layout.pane_ids().contains(&sessions_id));
    assert_eq!(app.focused, app.main_active_id());
}
