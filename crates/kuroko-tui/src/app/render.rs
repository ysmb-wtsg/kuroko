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
                } else if let Some(pane) = self.panes.get_mut(pane_id) {
                    pane.render(frame, *pane_area, *pane_id == self.focused);
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
    /// フォーカス中ペイン種別・グローバルレイヤー状態・エージェント状態・一時メッセージを集約表示する。
    fn render_status_bar(&self, frame: &mut ratatui::Frame, area: Rect) {
        let t = theme::get();

        let mut spans = Vec::new();

        // グローバルレイヤー中はバッジを先頭に表示する
        if self.global_layer {
            spans.push(Span::styled(
                " GLOBAL ",
                Style::default()
                    .fg(t.text_on_accent)
                    .bg(t.accent_warning)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        // フォーカス中ペインの種別バッジ（旧モード表示の代替）。
        // レイヤー状態のGLOBALバッジを目立たせるため、こちらは中立色のチップにする
        let pane_label = match self.panes.get(&self.focused).map(|p| p.pane_type()) {
            Some(PaneType::Agent) => "AGENT",
            Some(PaneType::Terminal) => "TERM",
            Some(PaneType::FileTree) => "FILES",
            None => "-",
        };
        spans.push(Span::styled(
            format!(" {pane_label} "),
            Style::default()
                .fg(t.text_primary)
                .bg(t.surface_highlight)
                .add_modifier(Modifier::BOLD),
        ));

        // フォーカス中ペインのタイトル
        if let Some(pane) = self.panes.get(&self.focused) {
            spans.push(Span::styled(
                format!(" {} ", pane.title()),
                Style::default().fg(t.text_muted),
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
            ("q / Esc", "Exit copy mode"),
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

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(t.accent_primary))
            .title(format!(" {} ", file_name))
            .style(Style::default().bg(t.surface_overlay));

        let inner = block.inner(preview_area);
        // 下のペイン内容が透けないよう領域を消去してから描画する
        frame.render_widget(Clear, preview_area);
        frame.render_widget(block, preview_area);

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

        // スクロール位置の補正
        let max_scroll = total_lines.saturating_sub(inner.height as usize);
        let scroll = preview.scroll.min(max_scroll);

        // 表示範囲の行を描画
        for (i, highlighted_line) in preview
            .lines
            .iter()
            .skip(scroll)
            .take(inner.height as usize)
            .enumerate()
        {
            let row = inner.y + i as u16;
            if row >= inner.y + inner.height {
                break;
            }

            let line = if preview.is_text {
                let line_num = scroll + i + 1;
                // 行番号 + ハイライト済みスパンを結合
                let mut spans = vec![Span::styled(
                    format!("{:>width$} ", line_num, width = line_num_width),
                    Style::default().fg(t.text_dim),
                )];
                for (text, style) in &highlighted_line.spans {
                    spans.push(Span::styled(text.clone(), *style));
                }
                Line::from(spans)
            } else {
                // バイナリ/エラー表示: HighlightedLineのスパンをそのまま使う
                let spans: Vec<Span> = highlighted_line
                    .spans
                    .iter()
                    .map(|(text, style)| Span::styled(text.clone(), *style))
                    .collect();
                Line::from(spans)
            };

            let line_area = Rect::new(inner.x, row, inner.width, 1);
            frame.render_widget(Paragraph::new(line).wrap(Wrap { trim: false }), line_area);
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
