//! App構造体の描画ロジック。
//! ペインのレイアウト描画、セパレータ、タブバー、ステータスバー、オーバーレイ描画を担当する。

use std::io;

use ratatui::DefaultTerminal;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use kuroko_agent::{AgentPane, AgentStatus};
use kuroko_core::layout::SplitDirection;
use kuroko_core::theme;
use kuroko_core::{FilePromptKind, PaneType};
use kuroko_terminal::TerminalPane;

use super::App;
use super::overlay::MessageLevel;
use super::tab_manager::TabManager;

impl App {
    /// ターミナルにフレームを描画する
    pub(super) fn draw(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        terminal.draw(|frame| {
            let area = frame.area();
            self.last_area = area;

            // ステータスバー用に1行確保
            let main_area = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
            let status_area = Rect::new(area.x, area.height.saturating_sub(1), area.width, 1);

            // レイアウト解決（メイン + ボトムターミナル + サイドパネル）
            let active_main = self.main_tabs.active_id().unwrap();
            let active_bottom = self.bottom_terminal_tabs.active_id();

            let (pane_areas, separators) = self.layout.resolve_with_separators(main_area);

            // フォーカス中ペインの領域（セパレータのフォーカス表示に使用）
            let focused_rect = pane_areas
                .iter()
                .find(|(id, _)| *id == self.focused)
                .map(|(_, rect)| *rect);

            // フォーカス中ペインの本体領域（カーソル表示に使用）
            let mut focused_body: Option<Rect> = None;

            for (pane_id, pane_area) in &pane_areas {
                let show_tab_bar = (*pane_id == active_main
                    || (self.bottom_visible && Some(*pane_id) == active_bottom))
                    && pane_area.height > 1;
                if show_tab_bar {
                    // タブバー + 本体。ボトムタブバーはタブ数に関わらず常に表示し、
                    // タブ追加時のレイアウトシフトを防ぐ。
                    let is_bottom = Some(*pane_id) == active_bottom && *pane_id != active_main;
                    let tab_bar_area = Rect::new(pane_area.x, pane_area.y, pane_area.width, 1);
                    let rename = if self.overlay.rename_input.is_some()
                        && self.overlay.renaming_bottom_tab == is_bottom
                    {
                        self.overlay.rename_input.as_deref()
                    } else {
                        None
                    };
                    let tab_mgr = if is_bottom {
                        &self.bottom_terminal_tabs
                    } else {
                        &self.main_tabs
                    };
                    self.render_tab_bar(frame, tab_bar_area, tab_mgr, rename);

                    let pane_body = Rect::new(
                        pane_area.x,
                        pane_area.y + 1,
                        pane_area.width,
                        pane_area.height.saturating_sub(1),
                    );
                    if let Some(pane) = self.panes.get_mut(pane_id) {
                        pane.render(frame, pane_body, *pane_id == self.focused);
                    }
                    if *pane_id == self.focused {
                        focused_body = Some(pane_body);
                    }
                } else if let Some(pane) = self.panes.get_mut(pane_id) {
                    pane.render(frame, *pane_area, *pane_id == self.focused);
                    if *pane_id == self.focused {
                        focused_body = Some(*pane_area);
                    }
                }
            }

            // 分割面のセパレータを描画（フォーカス中ペインに隣接する線は強調）
            self.render_separators(frame, &separators, focused_rect);

            // ステータスバーを描画
            self.render_status_bar(frame, status_area);

            // モーダルオーバーレイ描画
            if self.overlay.command_palette.is_some() {
                self.render_command_palette(frame, area);
            }
            if self.overlay.file_preview.is_some() {
                self.render_file_preview(frame, area);
            }
            if self.overlay.file_info.is_some() {
                self.render_file_info(frame, area);
            }
            if self.overlay.file_prompt.is_some() {
                self.render_file_prompt(frame, area);
            }
            if self.overlay.help_visible {
                self.render_help(frame, area);
            }

            // フォーカス中ペインの端末カーソルを表示する。
            // 直通状態でオーバーレイ・タブリネームが無いときのみ（グローバルレイヤー中は
            // キーがペインに行かないため、誤解を避けてカーソルを出さない）。
            let overlay_active = self.overlay.command_palette.is_some()
                || self.overlay.file_preview.is_some()
                || self.overlay.file_info.is_some()
                || self.overlay.file_prompt.is_some()
                || self.overlay.help_visible
                || self.overlay.rename_input.is_some();
            if !self.global_layer
                && !overlay_active
                && let Some(body) = focused_body
                && let Some(pane) = self.panes.get(&self.focused)
                && let Some(pos) = pane.cursor_position(body)
            {
                frame.set_cursor_position(pos);
            }
        })?;

        Ok(())
    }

    /// ペイン分割面のセパレータ線を描画する。
    /// フォーカス中ペインに隣接するセパレータは強調色で表示する。
    ///
    /// @param frame - 描画フレーム
    /// @param separators - セパレータの方向と領域のリスト
    /// @param focused_rect - フォーカス中ペインの領域
    fn render_separators(
        &self,
        frame: &mut ratatui::Frame,
        separators: &[(SplitDirection, Rect)],
        focused_rect: Option<Rect>,
    ) {
        let t = theme::get();
        for (direction, rect) in separators {
            let adjacent = focused_rect
                .map(|fr| is_separator_adjacent(*direction, *rect, fr))
                .unwrap_or(false);
            let color = if adjacent {
                t.border_focus
            } else {
                t.border_subtle
            };
            let style = Style::default().fg(color);
            let lines: Vec<Line> = match direction {
                SplitDirection::Vertical => (0..rect.height).map(|_| Line::from("│")).collect(),
                SplitDirection::Horizontal => {
                    vec![Line::from("─".repeat(rect.width as usize))]
                }
            };
            frame.render_widget(Paragraph::new(lines).style(style), *rect);
        }
    }

    /// タブバーを描画する（メインタブ・ボトムタブ共用）。
    /// `rename_input`が`Some`の場合、アクティブタブの名前部分をインライン編集表示にする。
    /// エージェントペインのタブにはステータスドット（Working/Exited）を表示する。
    ///
    /// @param frame - 描画フレーム
    /// @param area - タブバーの描画領域
    /// @param tab_mgr - タブ管理構造体
    /// @param rename_input - リネーム中の入力テキスト（リネーム中でなければNone）
    fn render_tab_bar(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        tab_mgr: &TabManager,
        rename_input: Option<&str>,
    ) {
        let t = theme::get();
        let tabs = tab_mgr.tabs();
        let active_index = tab_mgr.active_index();
        let names = tab_mgr.names();

        let mut spans = Vec::new();
        // リネーム入力カーソルの表示位置（確定したらSome）
        let mut cursor_x: Option<u16> = None;
        // 構築済みスパンの表示幅合計
        let mut width_so_far: usize = 0;

        /// スパンを追加しつつ表示幅を積算する
        fn push(spans: &mut Vec<Span<'static>>, width: &mut usize, span: Span<'static>) {
            *width += span.width();
            spans.push(span);
        }

        for (i, tab_id) in tabs.iter().enumerate() {
            let is_active = i == active_index;
            let is_renaming = is_active && rename_input.is_some();

            let current_name = names.get(i).and_then(|n| n.clone()).unwrap_or_else(|| {
                self.panes
                    .get(tab_id)
                    .map(|p| p.title().to_string())
                    .unwrap_or_else(|| "?".to_string())
            });

            // エージェントペインのステータスドット
            let status_dot = self
                .panes
                .get(tab_id)
                .and_then(|p| p.as_any().downcast_ref::<AgentPane>())
                .and_then(|ap| match ap.status() {
                    AgentStatus::Working => Some(("● ", t.accent_warning)),
                    AgentStatus::Exited => Some(("● ", t.text_dim)),
                    AgentStatus::Starting | AgentStatus::Idle => None,
                });

            if is_renaming {
                let input = rename_input.unwrap_or("").to_string();
                let bg = t.surface_active;
                push(
                    &mut spans,
                    &mut width_so_far,
                    Span::styled(
                        format!(" {}:", i + 1),
                        Style::default()
                            .fg(t.text_primary)
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                );
                if input.is_empty() {
                    // カーソルはプレースホルダーの先頭に置く
                    cursor_x = Some(area.x + width_so_far as u16);
                    push(
                        &mut spans,
                        &mut width_so_far,
                        Span::styled(
                            "Enter name... ".to_string(),
                            Style::default().fg(t.text_placeholder).bg(bg),
                        ),
                    );
                } else {
                    push(
                        &mut spans,
                        &mut width_so_far,
                        Span::styled(
                            input,
                            Style::default()
                                .fg(t.accent_warning)
                                .bg(bg)
                                .add_modifier(Modifier::BOLD),
                        ),
                    );
                    cursor_x = Some(area.x + width_so_far as u16);
                    push(
                        &mut spans,
                        &mut width_so_far,
                        Span::styled(" ".to_string(), Style::default().bg(bg)),
                    );
                }
            } else {
                let style = if is_active {
                    Style::default()
                        .fg(t.text_primary)
                        .bg(t.surface_active)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.text_muted).bg(t.surface)
                };
                push(
                    &mut spans,
                    &mut width_so_far,
                    Span::styled(format!(" {}:", i + 1), style),
                );
                if let Some((dot, color)) = status_dot {
                    push(
                        &mut spans,
                        &mut width_so_far,
                        Span::styled(dot.to_string(), style.fg(color)),
                    );
                }
                push(
                    &mut spans,
                    &mut width_so_far,
                    Span::styled(format!("{} ", current_name), style),
                );
            }
            if i + 1 < tabs.len() {
                push(
                    &mut spans,
                    &mut width_so_far,
                    Span::styled("│".to_string(), Style::default().fg(t.border_subtle)),
                );
            }
        }
        let line = Line::from(spans);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(t.surface)),
            area,
        );
        // リネーム中は実カーソルを入力末尾に表示する
        if let Some(x) = cursor_x {
            frame.set_cursor_position((x.min(area.right().saturating_sub(1)), area.y));
        }
    }

    /// ステータスバーを描画する。
    /// フォーカス中ペインのタイトル・グローバルレイヤー状態・エージェント状態・一時メッセージを集約表示する。
    fn render_status_bar(&self, frame: &mut ratatui::Frame, area: Rect) {
        let t = theme::get();

        let mut spans = Vec::new();

        // 先頭バッジ: グローバルレイヤー中はGLOBAL、コピーモード中はCOPYを表示する。
        // どちらもアプリの「特殊な入力状態」を表し相互排他なので、同じスロット・同じ
        // スタイルで入れ替えて表示する（コピーモード中はglobal_layerは必ずfalse）。
        let copy_offset = self
            .panes
            .get(&self.focused)
            .and_then(|p| {
                let any = p.as_any();
                any.downcast_ref::<TerminalPane>()
                    .map(|tp| (tp.is_copy_mode(), tp.scroll_offset()))
                    .or_else(|| {
                        any.downcast_ref::<AgentPane>()
                            .map(|ap| (ap.is_copy_mode(), ap.scroll_offset()))
                    })
            })
            .filter(|(in_copy, _)| *in_copy)
            .map(|(_, offset)| offset);
        let leading_badge = if self.global_layer {
            Some(" GLOBAL ".to_string())
        } else {
            copy_offset.map(|offset| {
                if offset > 0 {
                    format!(" COPY +{offset} ")
                } else {
                    " COPY ".to_string()
                }
            })
        };
        if let Some(label) = leading_badge {
            spans.push(Span::styled(
                label,
                Style::default()
                    .fg(t.text_on_accent)
                    .bg(t.accent_warning)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        // フォーカス中ペインのタイトルを種別ごとの色付きバッジで表示する
        if let Some(pane) = self.panes.get(&self.focused) {
            let bg = match pane.pane_type() {
                PaneType::Agent => t.accent_primary,
                PaneType::Terminal => t.accent_positive,
                PaneType::FileTree => t.accent_secondary,
            };
            spans.push(Span::styled(
                format!(" {} ", pane.title()),
                Style::default()
                    .fg(t.text_on_accent)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        // エージェント状態の集約表示（非フォーカスのエージェントも対象）
        let mut working = 0_usize;
        let mut exited = 0_usize;
        for pane in self.panes.values() {
            if let Some(ap) = pane.as_any().downcast_ref::<AgentPane>() {
                match ap.status() {
                    AgentStatus::Working => working += 1,
                    AgentStatus::Exited => exited += 1,
                    AgentStatus::Starting | AgentStatus::Idle => {}
                }
            }
        }
        if working > 0 {
            spans.push(Span::styled(
                " ● working ",
                Style::default().fg(t.accent_warning),
            ));
        } else if exited > 0 {
            spans.push(Span::styled(" ● exited ", Style::default().fg(t.text_dim)));
        }

        // 一時メッセージ（重要度に応じた色で表示）
        if let Some(ref msg) = self.overlay.status_message {
            let color = match msg.level {
                MessageLevel::Info => t.text_body,
                MessageLevel::Warn => t.accent_warning,
                MessageLevel::Error => t.accent_error,
            };
            spans.push(Span::styled(
                format!("  {}", msg.text),
                Style::default().fg(color),
            ));
        }

        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(t.surface)),
            area,
        );

        // 右端にヘルプコマンドのヒントを表示（グローバルレイヤー中のみ。
        // 直通中は`:`がペインに送られるため誤誘導になる）
        if self.global_layer {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    " :help ",
                    Style::default().fg(t.text_dim).bg(t.surface),
                ))
                .alignment(ratatui::layout::Alignment::Right),
                area,
            );
        }
    }

    /// キーバインドのヘルプをフローティングウィンドウとして描画する。
    /// 組み込みキーマップのチートシートを表示する（Luaカスタムキーマップは含まない）。
    fn render_help(&self, frame: &mut ratatui::Frame, area: Rect) {
        let t = theme::get();

        // (キー, 説明) のリスト。空キーはセクション見出し
        let entries: &[(&str, &str)] = &[
            ("", "Global layer"),
            ("C-Space", "Enter / leave global layer"),
            ("Esc / i", "Back to direct input"),
            ("h/j/k/l", "Focus pane left/down/up/right"),
            ("Tab / S-Tab", "Focus next / previous pane"),
            ("H/J/K/L", "Resize pane"),
            ("t / f / g", "Toggle terminal / files / git panel"),
            ("n", "New tab"),
            ("x or w", "Close tab"),
            ("r", "Rename tab"),
            ("] / [", "Next / previous tab"),
            ("1-9", "Select tab by number"),
            ("Enter", "Copy mode (terminal/agent pane)"),
            (":", "Command palette"),
            ("q", "Quit"),
            ("", "Agent / Terminal (direct input)"),
            ("Enter", "Send / submit"),
            (
                "Ctrl+j",
                "Insert a newline (Shift/Alt+Enter on supported terminals)",
            ),
            ("", "File tree (focused, direct input)"),
            ("j/k", "Move cursor"),
            ("Enter/l/h", "Expand / collapse directory"),
            ("p / i", "Preview / file info"),
            ("a / r / d", "Create / rename / delete"),
            ("o", "Send file path to agent"),
            ("y / Y", "Copy name / full path"),
            ("v", "Multi-select (Space:toggle d:delete)"),
            ("S-h", "Toggle hidden files"),
            ("", "Copy mode"),
            ("h/j/k/l", "Move cursor"),
            ("C-d / C-u", "Half page down / up"),
            ("g / G", "Scroll to top / bottom"),
            ("v", "Start / clear selection"),
            ("y", "Copy selection and exit"),
            ("Enter / q / Esc", "Exit copy mode"),
        ];

        let width = 56_u16.min(area.width);
        let height = (entries.len() as u16 + 2).min(area.height);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let help_area = Rect::new(x, y, width, height);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(t.accent_primary))
            .title(" Help ")
            .title_bottom(Line::from(Span::styled(
                " Esc/q:close ",
                Style::default().fg(t.text_dim),
            )))
            .style(Style::default().bg(t.surface_overlay));
        let inner = block.inner(help_area);
        // 下のペイン内容が透けないよう領域を消去してから描画する
        frame.render_widget(Clear, help_area);
        frame.render_widget(block, help_area);

        for (i, (key, desc)) in entries.iter().enumerate() {
            let row = inner.y + i as u16;
            if row >= inner.y + inner.height {
                break;
            }
            let line = if key.is_empty() {
                // セクション見出し
                Line::from(Span::styled(
                    desc.to_string(),
                    Style::default()
                        .fg(t.accent_primary)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(vec![
                    Span::styled(
                        format!("  {:<13}", key),
                        Style::default().fg(t.text_primary),
                    ),
                    Span::styled(desc.to_string(), Style::default().fg(t.text_muted)),
                ])
            };
            let line_area = Rect::new(inner.x, row, inner.width, 1);
            frame.render_widget(Paragraph::new(line), line_area);
        }
    }

    /// ファイルプレビューをフローティングウィンドウとして描画する。
    ///
    /// @param frame - 描画フレーム
    /// @param area - 画面全体の領域
    fn render_file_preview(&self, frame: &mut ratatui::Frame, area: Rect) {
        let t = theme::get();
        let preview = match &self.overlay.file_preview {
            Some(p) => p,
            None => return,
        };

        // プレビューウィンドウのサイズと位置（画面中央に配置）
        let width = (area.width * 3 / 4).clamp(40, 100).min(area.width);
        let height = (area.height * 3 / 4).max(10).min(area.height);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let preview_area = Rect::new(x, y, width, height);

        // ファイル名をタイトルに表示
        let file_name = preview
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| preview.path.to_string_lossy().to_string());

        // 枠線は描かない。端末のワイド文字表示幅（em dash等のEast Asian Ambiguous）と
        // ratatuiのunicode-widthの不一致で本文行がずれ、枠がある限り右枠が構造的に乱れる
        // ため。背景色を敷いたフローティングパネルとし、上端にファイル名、その下に本文を描く。
        let block = Block::default()
            .borders(Borders::NONE)
            .style(Style::default().bg(t.surface_overlay));

        // 下のペイン内容が透けないよう領域を消去してから背景を描く
        frame.render_widget(Clear, preview_area);
        frame.render_widget(block, preview_area);

        // 上端1行にファイル名を表示
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {} ", file_name),
                Style::default().fg(t.accent_primary).bg(t.surface_overlay),
            ))),
            Rect::new(preview_area.x, preview_area.y, preview_area.width, 1),
        );

        // 本文領域: 上端1行をタイトルに使い、左右に1桁の余白を取る。
        // 右余白により、ワイド文字の幅ズレで本文が右へずれてもパネル内に収まりやすい。
        let inner = Rect {
            x: preview_area.x + 1,
            y: preview_area.y + 1,
            width: preview_area.width.saturating_sub(2),
            height: preview_area.height.saturating_sub(1),
        };

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        // 行番号の桁数を計算
        let total_lines = preview.lines.len();
        let line_num_width = if preview.is_text {
            total_lines.to_string().len()
        } else {
            0
        };

        // スクロール位置の補正（先頭表示するソース行のインデックス）
        let max_scroll = total_lines.saturating_sub(1);
        let scroll = preview.scroll.min(max_scroll);

        // 行番号の固定幅プレフィックス（行番号 + 空白1）。本文はこの右側に折り返し描画する。
        let prefix_width = if preview.is_text {
            (line_num_width + 1) as u16
        } else {
            0
        };
        let content_x = inner.x + prefix_width;
        let content_width = inner.width.saturating_sub(prefix_width);

        // 表示範囲のソース行を、横に長い行は折り返して描画する。
        // 1ソース行が複数の表示行に跨るため、行カーソルを折り返し行数ぶん進める。
        // 行番号は折り返しブロックの先頭行にのみ表示する。
        let end_row = inner.y + inner.height;
        let mut row = inner.y;
        let mut src_idx = scroll;
        while row < end_row && src_idx < total_lines {
            let highlighted_line = &preview.lines[src_idx];
            let content_spans: Vec<Span> = highlighted_line
                .spans
                .iter()
                .map(|(text, style)| Span::styled(text.clone(), *style))
                .collect();
            let para = Paragraph::new(Line::from(content_spans)).wrap(Wrap { trim: false });

            // 折り返し後の表示行数を求め、残り領域内に収める
            let wrapped_rows = (para.line_count(content_width).max(1) as u16).min(end_row - row);

            if preview.is_text {
                let line_num = src_idx + 1;
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        format!("{:>width$} ", line_num, width = line_num_width),
                        Style::default().fg(t.text_dim),
                    ))),
                    Rect::new(inner.x, row, prefix_width, 1),
                );
            }

            frame.render_widget(para, Rect::new(content_x, row, content_width, wrapped_rows));

            row += wrapped_rows;
            src_idx += 1;
        }

        // スクロールインジケーター
        if total_lines > inner.height as usize {
            let scroll_info = format!(" {}/{} ", scroll + 1, total_lines);
            let info_width = scroll_info.len() as u16;
            let info_area = Rect::new(
                preview_area.x + preview_area.width.saturating_sub(info_width + 1),
                preview_area.y,
                info_width,
                1,
            );
            frame.render_widget(
                Paragraph::new(Span::styled(
                    scroll_info,
                    Style::default().fg(t.text_muted).bg(t.surface_overlay),
                )),
                info_area,
            );
        }
    }

    /// ファイル操作プロンプトをオーバーレイ描画する。
    /// 削除確認のみ警告色の枠、それ以外はプライマリ色の枠で表示する。
    fn render_file_prompt(&self, frame: &mut ratatui::Frame, area: Rect) {
        let t = theme::get();
        let prompt = match &self.overlay.file_prompt {
            Some(p) => p,
            None => return,
        };

        let (label, hint) = match &prompt.kind {
            FilePromptKind::Create { .. } => ("New file: ", "(trailing / for directory)"),
            FilePromptKind::Rename { .. } => ("Rename: ", ""),
            FilePromptKind::Delete { .. } => ("Delete? (y/n): ", ""),
        };

        // Delete は破壊的操作なので警告色で区別する
        let is_delete = matches!(&prompt.kind, FilePromptKind::Delete { .. });
        let accent = if is_delete {
            t.accent_warning
        } else {
            t.accent_primary
        };

        // Delete の場合はファイル名を表示
        let display_text = match &prompt.kind {
            FilePromptKind::Delete { paths } => {
                if paths.len() == 1 {
                    paths[0]
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default()
                } else {
                    format!("{} items", paths.len())
                }
            }
            _ => prompt.input.clone(),
        };

        let width = area.width.min(60);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = area.height / 3;
        let prompt_area = Rect::new(x, y, width, 3.min(area.height));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(accent))
            .style(Style::default().bg(t.surface_overlay));
        let inner = block.inner(prompt_area);
        // 下のペイン内容が透けないよう領域を消去してから描画する
        frame.render_widget(Clear, prompt_area);
        frame.render_widget(block, prompt_area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let line = Line::from(vec![
            Span::styled(
                label,
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(&display_text, Style::default().fg(t.text_primary)),
            Span::styled(format!("  {hint}"), Style::default().fg(t.text_dim)),
        ]);
        let line_area = Rect::new(inner.x, inner.y, inner.width, 1);
        frame.render_widget(Paragraph::new(line), line_area);

        // 入力を伴うプロンプトは実カーソルを入力末尾に表示する
        if !is_delete {
            let cursor_x = inner.x
                + (Span::raw(label).width() + Span::raw(display_text.as_str()).width()) as u16;
            frame.set_cursor_position((cursor_x.min(inner.right().saturating_sub(1)), inner.y));
        }
    }

    /// コマンドパレットをフローティング描画する。
    /// 入力欄と候補リストを中央に表示し、選択中の候補をハイライトする。
    fn render_command_palette(&self, frame: &mut ratatui::Frame, area: Rect) {
        let t = theme::get();
        let palette = match &self.overlay.command_palette {
            Some(p) => p,
            None => return,
        };

        let max_visible = 10_u16;
        let list_height = (palette.filtered.len() as u16).min(max_visible);
        // 入力行(1) + ボーダー(2) + リスト行
        let height = (list_height + 3).min(area.height);
        let width = area.width.min(60);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = area.height / 4;
        let palette_area = Rect::new(x, y, width, height);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(t.accent_primary))
            .title(" Command ")
            .style(Style::default().bg(t.surface_overlay));
        let inner = block.inner(palette_area);
        // 下のペイン内容が透けないよう領域を消去してから描画する
        frame.render_widget(Clear, palette_area);
        frame.render_widget(block, palette_area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        // 入力行（実カーソルを入力末尾に表示）
        let prompt_span = Span::styled(
            ": ",
            Style::default()
                .fg(t.accent_primary)
                .add_modifier(Modifier::BOLD),
        );
        let input_span = Span::styled(palette.input.clone(), Style::default().fg(t.text_primary));
        let cursor_x = inner.x + (prompt_span.width() + input_span.width()) as u16;
        let input_line = Line::from(vec![prompt_span, input_span]);
        let input_area = Rect::new(inner.x, inner.y, inner.width, 1);
        frame.render_widget(Paragraph::new(input_line), input_area);
        frame.set_cursor_position((cursor_x.min(inner.right().saturating_sub(1)), inner.y));

        // 候補リスト
        for (i, &cmd_idx) in palette
            .filtered
            .iter()
            .take(inner.height.saturating_sub(1) as usize)
            .enumerate()
        {
            let row = inner.y + 1 + i as u16;
            if row >= inner.y + inner.height {
                break;
            }
            let cmd = &palette.commands[cmd_idx];
            let is_selected = i == palette.selected;

            let (name_style, desc_style) = if is_selected {
                (
                    Style::default()
                        .fg(t.text_primary)
                        .bg(t.surface_active)
                        .add_modifier(Modifier::BOLD),
                    Style::default().fg(t.text_muted).bg(t.surface_active),
                )
            } else {
                (
                    Style::default().fg(t.text_primary),
                    Style::default().fg(t.text_muted),
                )
            };

            let line = Line::from(vec![
                Span::styled(format!("  {} ", cmd.name), name_style),
                Span::styled(format!(" {}", cmd.description), desc_style),
            ]);
            let line_area = Rect::new(inner.x, row, inner.width, 1);
            frame.render_widget(Paragraph::new(line), line_area);
        }
    }

    /// ファイル詳細情報をフローティング描画する。
    fn render_file_info(&self, frame: &mut ratatui::Frame, area: Rect) {
        let t = theme::get();
        let info = match &self.overlay.file_info {
            Some(i) => i,
            None => return,
        };

        let height = (info.lines.len() as u16 + 2).min(area.height / 2);
        let width = area.width.min(60);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let info_area = Rect::new(x, y, width, height);

        let file_name = info
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(t.accent_primary))
            .title(format!(" {file_name} "))
            .title_bottom(Line::from(Span::styled(
                " Esc/i:close ",
                Style::default().fg(t.text_dim),
            )))
            .style(Style::default().bg(t.surface_overlay));
        let inner = block.inner(info_area);
        // 下のペイン内容が透けないよう領域を消去してから描画する
        frame.render_widget(Clear, info_area);
        frame.render_widget(block, info_area);

        for (i, (key, value)) in info.lines.iter().enumerate() {
            let row = inner.y + i as u16;
            if row >= inner.y + inner.height {
                break;
            }
            let line = Line::from(vec![
                Span::styled(
                    format!("{:>12}: ", key),
                    Style::default().fg(t.accent_primary),
                ),
                Span::styled(value, Style::default().fg(t.text_body)),
            ]);
            let line_area = Rect::new(inner.x, row, inner.width, 1);
            frame.render_widget(Paragraph::new(line), line_area);
        }
    }
}

/// セパレータがフォーカス中ペインの領域に隣接しているかを判定する。
///
/// @param direction - セパレータの方向
/// @param sep - セパレータの領域
/// @param focused - フォーカス中ペインの領域
/// @returns 隣接していればtrue
fn is_separator_adjacent(direction: SplitDirection, sep: Rect, focused: Rect) -> bool {
    match direction {
        SplitDirection::Vertical => {
            let touches = focused.right() == sep.x || sep.x + 1 == focused.x;
            let overlaps = focused.y < sep.bottom() && sep.y < focused.bottom();
            touches && overlaps
        }
        SplitDirection::Horizontal => {
            let touches = focused.bottom() == sep.y || sep.y + 1 == focused.y;
            let overlaps = focused.x < sep.right() && sep.x < focused.right();
            touches && overlaps
        }
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;

    #[test]
    fn vertical_separator_adjacent_to_left_pane() {
        // フォーカスペイン(0,0,49,50)の右端 x=49 にセパレータがある場合
        let sep = Rect::new(49, 0, 1, 50);
        let focused = Rect::new(0, 0, 49, 50);
        assert!(is_separator_adjacent(
            SplitDirection::Vertical,
            sep,
            focused
        ));
    }

    #[test]
    fn vertical_separator_adjacent_to_right_pane() {
        let sep = Rect::new(49, 0, 1, 50);
        let focused = Rect::new(50, 0, 50, 50);
        assert!(is_separator_adjacent(
            SplitDirection::Vertical,
            sep,
            focused
        ));
    }

    #[test]
    fn vertical_separator_not_adjacent_when_apart() {
        let sep = Rect::new(49, 0, 1, 50);
        let focused = Rect::new(60, 0, 20, 50);
        assert!(!is_separator_adjacent(
            SplitDirection::Vertical,
            sep,
            focused
        ));
    }

    #[test]
    fn horizontal_separator_adjacent_to_top_pane() {
        let sep = Rect::new(0, 24, 100, 1);
        let focused = Rect::new(0, 0, 100, 24);
        assert!(is_separator_adjacent(
            SplitDirection::Horizontal,
            sep,
            focused
        ));
    }

    #[test]
    fn horizontal_separator_not_adjacent_without_overlap() {
        // 上下では接していてもX範囲が重ならなければ隣接とみなさない
        let sep = Rect::new(50, 24, 50, 1);
        let focused = Rect::new(0, 0, 49, 24);
        assert!(!is_separator_adjacent(
            SplitDirection::Horizontal,
            sep,
            focused
        ));
    }
}
