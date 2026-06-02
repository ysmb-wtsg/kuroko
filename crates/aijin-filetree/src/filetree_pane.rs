//! ファイルツリーペインの実装。
//! Paneトレイトを実装し、ディレクトリツリーの表示・操作・SELECT選択を管理する。

use std::any::Any;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use aijin_core::theme;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

use aijin_core::{Action, AppEvent, FilePromptKind, Mode, Pane, PaneId, PaneType};

use crate::icons::file_icon;
use crate::tree::{FlatEntry, TreeEntry};

/// ファイルツリーペイン。
/// ディレクトリツリーを表示し、ファイル選択操作・SELECT複数選択を提供する。
pub struct FileTreePane {
    /// ペインの一意ID
    id: PaneId,
    /// ツリーのルートエントリ
    root: TreeEntry,
    /// リスト選択状態（ratatui用）
    list_state: ListState,
    /// 現在のカーソル位置（フラットリスト上のインデックス）
    selected: usize,
    /// SELECTモードで選択されたアイテムのインデックス群
    selected_items: HashSet<usize>,
    /// ツリーをフラット展開したキャッシュ。ツリー変更時に再構築する。
    flat_cache: Vec<FlatEntry>,
    /// 隠しファイルを表示するかどうか（デフォルト: false = 非表示）
    show_hidden: bool,
}

impl FileTreePane {
    /// 指定パスをルートとするFileTreePaneを生成する。
    ///
    /// @param id - ペインID
    /// @param path - ルートディレクトリのパス
    /// @returns FileTreePaneインスタンス
    pub fn new(id: PaneId, path: PathBuf) -> Self {
        let show_hidden = false;
        let root = TreeEntry::new_root(&path, show_hidden);
        let flat_cache = root.flatten();
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Self {
            id,
            root,
            list_state,
            selected: 0,
            selected_items: HashSet::new(),
            flat_cache,
            show_hidden,
        }
    }

    /// 選択中のファイルパスを取得する
    pub fn selected_path(&self) -> Option<PathBuf> {
        self.flat_cache.get(self.selected).map(|e| e.path.clone())
    }

    /// カーソルを上に移動
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.list_state.select(Some(self.selected));
        }
    }

    /// カーソルを下に移動
    pub fn move_down(&mut self) {
        if self.selected + 1 < self.flat_cache.len() {
            self.selected += 1;
            self.list_state.select(Some(self.selected));
        }
    }

    /// 選択中のエントリを展開/折りたたみし、フラットキャッシュを再構築する
    fn toggle_selected(&mut self) {
        self.root.toggle(self.selected, self.show_hidden);
        self.flat_cache = self.root.flatten();
    }

    /// SELECTモード: 現在のカーソル位置の選択状態をトグルする
    pub fn toggle_selection(&mut self) {
        if self.selected_items.contains(&self.selected) {
            self.selected_items.remove(&self.selected);
        } else {
            self.selected_items.insert(self.selected);
        }
    }

    /// SELECTモード: 全選択を解除する
    pub fn clear_selections(&mut self) {
        self.selected_items.clear();
    }

    /// SELECTモード: 選択されたアイテムのパスリストを返す
    ///
    /// @returns 選択中の全ファイル/ディレクトリのパス
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected_items.iter()
            .filter_map(|&idx| self.flat_cache.get(idx).map(|e| e.path.clone()))
            .collect()
    }

    /// カーソル位置から操作対象の親ディレクトリを判定する。
    /// カーソルがディレクトリ上 → そのディレクトリ、ファイル上 → その親ディレクトリ。
    ///
    /// @returns 親ディレクトリのパス
    pub fn parent_dir_of_selected(&self) -> Option<PathBuf> {
        self.flat_cache.get(self.selected).map(|entry| {
            if entry.is_dir {
                entry.path.clone()
            } else {
                entry.path.parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| entry.path.clone())
            }
        })
    }

    /// ツリーを再読み込みする。カーソル位置はエントリ総数内に収める。
    ///
    /// @param target - リフレッシュ対象のパス（そのパスを含むサブツリーを再読み込み）
    pub fn refresh(&mut self, target: &Path) {
        self.root.refresh_subtree(target, self.show_hidden);
        self.flat_cache = self.root.flatten();
        // カーソルがエントリ数を超えていたら末尾に補正
        if self.selected >= self.flat_cache.len() {
            self.selected = self.flat_cache.len().saturating_sub(1);
        }
        self.list_state.select(Some(self.selected));
        // リフレッシュ後はSELECTのインデックスが無効になるため解除
        self.selected_items.clear();
    }

    /// 選択済みアイテムがあるかどうかを返す
    pub fn has_selections(&self) -> bool {
        !self.selected_items.is_empty()
    }

    /// 隠しファイルの表示/非表示をトグルし、ツリーを再構築する
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        // ルートパスを保持してツリー全体を再構築する
        let root_path = self.root.path.clone();
        self.root = TreeEntry::new_root(&root_path, self.show_hidden);
        self.flat_cache = self.root.flatten();
        // カーソルがエントリ数を超えていたら末尾に補正
        if self.selected >= self.flat_cache.len() {
            self.selected = self.flat_cache.len().saturating_sub(1);
        }
        self.list_state.select(Some(self.selected));
        self.selected_items.clear();
    }
}

impl Pane for FileTreePane {
    fn id(&self) -> PaneId {
        self.id
    }

    fn title(&self) -> &str {
        "Files"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        let t = theme::get();
        // ボーダーは描画せず、左右1セルの余白のみ確保する
        let inner = area.inner(Margin { horizontal: 1, vertical: 0 });

        let items: Vec<ListItem> = self.flat_cache
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let indent = "  ".repeat(entry.depth);
                let fi = file_icon(&entry.name, entry.is_dir, entry.expanded);
                let name_style = if entry.is_dir {
                    Style::default().fg(t.accent_primary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.text_body)
                };

                // SELECTモードの選択マーカー
                let is_selected = self.selected_items.contains(&idx);
                let marker = if is_selected { "\u{f00c} " } else { "" };
                let marker_style = Style::default().fg(t.accent_positive);

                let mut spans = vec![Span::raw(indent)];
                if is_selected {
                    spans.push(Span::styled(marker, marker_style));
                }
                spans.push(Span::styled(format!("{} ", fi.icon), Style::default().fg(fi.color)));
                spans.push(Span::styled(&entry.name, name_style));

                ListItem::new(Line::from(spans))
            })
            .collect();

        let highlight_style = if focused {
            Style::default().bg(t.surface_active).add_modifier(Modifier::BOLD)
        } else {
            Style::default().bg(t.surface_highlight)
        };

        let list = List::new(items).highlight_style(highlight_style);

        frame.render_stateful_widget(list, inner, &mut self.list_state);
    }

    fn handle_event(&mut self, event: &AppEvent) -> Vec<Action> {
        use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

        if let AppEvent::Key(key) = event {
            if key.kind != KeyEventKind::Press {
                return vec![];
            }
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => self.move_down(),
                KeyCode::Char('k') | KeyCode::Up => self.move_up(),
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    if let Some(entry) = self.flat_cache.get(self.selected)
                        && entry.is_dir {
                            self.toggle_selected();
                        }
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    if let Some(entry) = self.flat_cache.get(self.selected)
                        && entry.is_dir && entry.expanded {
                            self.toggle_selected();
                        }
                }
                KeyCode::Char('p') => {
                    if let Some(path) = self.selected_path() {
                        return vec![Action::ToggleFilePreview(path)];
                    }
                }
                KeyCode::Char('a') => {
                    if let Some(parent_dir) = self.parent_dir_of_selected() {
                        return vec![Action::OpenFilePrompt(
                            FilePromptKind::Create { parent_dir },
                        )];
                    }
                }
                KeyCode::Char('d') => {
                    if let Some(path) = self.selected_path() {
                        return vec![Action::OpenFilePrompt(
                            FilePromptKind::Delete { paths: vec![path] },
                        )];
                    }
                }
                KeyCode::Char('r') => {
                    if let Some(path) = self.selected_path() {
                        let current_name = path.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        return vec![Action::OpenFilePrompt(
                            FilePromptKind::Rename { path, current_name },
                        )];
                    }
                }
                KeyCode::Char('o') => {
                    if let Some(entry) = self.flat_cache.get(self.selected)
                        && !entry.is_dir {
                            return vec![Action::SendFileToAgent(entry.path.clone())];
                        }
                }
                KeyCode::Char('i') => {
                    if let Some(path) = self.selected_path() {
                        return vec![Action::ShowFileInfo(path)];
                    }
                }
                KeyCode::Char('y') if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                    if let Some(entry) = self.flat_cache.get(self.selected) {
                        return vec![Action::CopyToClipboard(entry.name.clone())];
                    }
                }
                KeyCode::Char('Y') => {
                    if let Some(path) = self.selected_path() {
                        return vec![Action::CopyToClipboard(
                            path.to_string_lossy().to_string(),
                        )];
                    }
                }
                KeyCode::Char('v') => {
                    return vec![Action::SetMode(Mode::Select)];
                }
                KeyCode::Char('H') => {
                    // 隠しファイルの表示/非表示をトグル
                    self.toggle_hidden();
                }
                _ => {}
            }
        }
        vec![]
    }

    fn wants_raw_input(&self) -> bool {
        false
    }

    fn pane_type(&self) -> PaneType {
        PaneType::FileTree
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
