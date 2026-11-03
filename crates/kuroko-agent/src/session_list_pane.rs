//! セッション一覧サイドバー（垂直タブ）。
//! 上段に開いているメインタブ、下段に Claude Code の過去セッションを並べ、
//! 選択してEnterでタブ切り替え・セッション再開を行う。

use std::any::Any;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

use kuroko_core::theme;
use kuroko_core::{Action, AppEvent, Pane, PaneId, PaneType};

use crate::claude_sessions::{self, SessionInfo, TitleCache};
use crate::status::AgentStatus;

/// 履歴を自動で読み直す間隔（表示中のみ）
const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

/// 開いているメインタブの情報（App側から毎フレーム渡される）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSession {
    /// メインタブ内のインデックス（`Action::SelectTab` に渡す）
    pub tab_index: usize,
    /// タブの表示名
    pub name: String,
    /// タブで動いている Claude Code のセッションID（不明ならNone）
    pub session_id: Option<String>,
    /// エージェントの状態（エージェント以外のペインならNone）
    pub status: Option<AgentStatus>,
    /// アクティブなタブか
    pub active: bool,
}

/// 選択可能な行
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    /// 開いているタブ（`open` のインデックス）
    Open(usize),
    /// 過去セッション（`history` のインデックス）
    History(usize),
}

/// セッション一覧サイドバーのペイン
pub struct SessionListPane {
    id: PaneId,
    /// セッションjsonlの保存先（ホームディレクトリ不明ならNone）
    dir: Option<PathBuf>,
    /// 開いているタブ
    open: Vec<OpenSession>,
    /// 過去セッション（新しい順）
    history: Vec<SessionInfo>,
    /// 選択中の行（`entries()` 上のインデックス）
    selected: usize,
    list_state: ListState,
    /// タイトル解析結果のキャッシュ（読み込みスレッドと共有）
    cache: Arc<Mutex<TitleCache>>,
    /// 読み込み中スレッドからの結果受信口
    loading: Option<mpsc::Receiver<Vec<SessionInfo>>>,
    /// 最後に読み込みを開始した時刻
    last_refresh: Option<Instant>,
}

impl SessionListPane {
    /// 作業ディレクトリに対応するセッション一覧ペインを生成する
    ///
    /// @param id - ペインID
    /// @param cwd - 作業ディレクトリ（Claude Code のプロジェクト識別に使う）
    pub fn new(id: PaneId, cwd: PathBuf) -> Self {
        Self::with_dir(id, claude_sessions::project_sessions_dir(&cwd))
    }

    /// セッション保存先ディレクトリを直接指定して生成する
    fn with_dir(id: PaneId, dir: Option<PathBuf>) -> Self {
        Self {
            id,
            dir,
            open: Vec::new(),
            history: Vec::new(),
            selected: 0,
            list_state: ListState::default(),
            cache: Arc::new(Mutex::new(TitleCache::new())),
            loading: None,
            last_refresh: None,
        }
    }

    /// 開いているタブの情報を更新する（App側から毎フレーム呼ぶ）
    pub fn set_open_sessions(&mut self, open: Vec<OpenSession>) {
        if self.open != open {
            self.open = open;
            self.clamp_selection();
        }
    }

    /// 過去セッションをバックグラウンドで読み直す。
    /// 大きなjsonlの読み込みでUIを止めないよう、別スレッドで解析する。
    pub fn refresh(&mut self) {
        let Some(dir) = self.dir.clone() else {
            return;
        };
        if self.loading.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let cache = Arc::clone(&self.cache);
        std::thread::spawn(move || {
            let sessions = match cache.lock() {
                Ok(mut cache) => claude_sessions::list_sessions(&dir, &mut cache),
                Err(_) => Vec::new(),
            };
            let _ = tx.send(sessions);
        });
        self.loading = Some(rx);
        self.last_refresh = Some(Instant::now());
    }

    /// 読み込み結果が届いていれば反映し、間隔が空いていれば再読み込みを始める
    fn poll_refresh(&mut self) {
        if let Some(rx) = &self.loading {
            match rx.try_recv() {
                Ok(sessions) => {
                    self.history = sessions;
                    self.loading = None;
                    self.clamp_selection();
                }
                Err(mpsc::TryRecvError::Disconnected) => self.loading = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let due = self
            .last_refresh
            .is_none_or(|t| t.elapsed() >= REFRESH_INTERVAL);
        if due {
            self.refresh();
        }
    }

    /// 表示中の過去セッション（タブで開いているものは除く）のインデックス
    fn visible_history(&self) -> impl Iterator<Item = usize> + '_ {
        self.history.iter().enumerate().filter_map(|(i, s)| {
            let is_open = self
                .open
                .iter()
                .any(|o| o.session_id.as_deref() == Some(s.id.as_str()));
            (!is_open).then_some(i)
        })
    }

    /// 選択可能な行を表示順に返す
    fn entries(&self) -> Vec<Entry> {
        (0..self.open.len())
            .map(Entry::Open)
            .chain(self.visible_history().map(Entry::History))
            .collect()
    }

    fn clamp_selection(&mut self) {
        let len = self.entries().len();
        self.selected = self.selected.min(len.saturating_sub(1));
    }

    /// 選択行に対するEnterの動作
    fn activate(&self) -> Vec<Action> {
        match self.entries().get(self.selected) {
            Some(Entry::Open(i)) => vec![Action::SelectTab(self.open[*i].tab_index)],
            Some(Entry::History(i)) => {
                let session = &self.history[*i];
                vec![Action::ResumeSession {
                    id: session.id.clone(),
                    title: session.title.clone(),
                }]
            }
            None => vec![],
        }
    }
}

/// 更新日時を「5m」「3h」「2d」のような短い相対表記にする
fn relative_time(modified: SystemTime, now: SystemTime) -> String {
    let secs = now
        .duration_since(modified)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match secs {
        0..60 => "now".to_string(),
        60..3600 => format!("{}m", secs / 60),
        3600..86400 => format!("{}h", secs / 3600),
        86400..604800 => format!("{}d", secs / 86400),
        _ => format!("{}w", secs / 604800),
    }
}

impl Pane for SessionListPane {
    fn id(&self) -> PaneId {
        self.id
    }

    fn title(&self) -> &str {
        "Sessions"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        self.poll_refresh();

        let t = theme::get();
        let inner = area.inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        let header_style = Style::default()
            .fg(t.text_muted)
            .add_modifier(Modifier::BOLD);
        let now = SystemTime::now();

        let mut items: Vec<ListItem> = Vec::new();
        // 選択行の、見出しを含めた描画上の位置
        let mut selected_row = None;
        let entries = self.entries();

        items.push(ListItem::new(Line::styled("TABS", header_style)));
        for (n, entry) in entries.iter().enumerate() {
            if let Entry::History(_) = entry
                && !matches!(entries.get(n.wrapping_sub(1)), Some(Entry::History(_)))
            {
                items.push(ListItem::new(Line::raw("")));
                items.push(ListItem::new(Line::styled("HISTORY", header_style)));
            }
            if n == self.selected {
                selected_row = Some(items.len());
            }
            let line = match entry {
                Entry::Open(i) => {
                    let open = &self.open[*i];
                    let (mark, color) = match open.status {
                        Some(AgentStatus::Working) => ("●", t.accent_warning),
                        Some(AgentStatus::Idle) => ("●", t.accent_positive),
                        Some(AgentStatus::Starting) => ("○", t.text_muted),
                        Some(AgentStatus::Exited) => ("×", t.text_dim),
                        None => ("•", t.text_muted),
                    };
                    let name_style = if open.active {
                        Style::default()
                            .fg(t.text_primary)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(t.text_body)
                    };
                    Line::from(vec![
                        Span::styled(format!("{mark} "), Style::default().fg(color)),
                        Span::styled(open.name.clone(), name_style),
                    ])
                }
                Entry::History(i) => {
                    let session = &self.history[*i];
                    Line::from(vec![
                        Span::styled(
                            format!("{:>3} ", relative_time(session.modified, now)),
                            Style::default().fg(t.text_dim),
                        ),
                        Span::styled(session.title.clone(), Style::default().fg(t.text_body)),
                    ])
                }
            };
            items.push(ListItem::new(line));
        }
        if self.dir.is_none() {
            items.push(ListItem::new(Line::styled(
                "(session directory not found)",
                Style::default().fg(t.text_dim),
            )));
        }

        let highlight_style = if focused {
            Style::default()
                .bg(t.surface_active)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().bg(t.surface_highlight)
        };
        self.list_state.select(selected_row);
        let list = List::new(items).highlight_style(highlight_style);
        frame.render_stateful_widget(list, inner, &mut self.list_state);
    }

    fn handle_event(&mut self, event: &AppEvent) -> Vec<Action> {
        use ratatui::crossterm::event::{KeyCode, KeyEventKind};

        let AppEvent::Key(key) = event else {
            return vec![];
        };
        if key.kind != KeyEventKind::Press {
            return vec![];
        }
        let last = self.entries().len().saturating_sub(1);
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.selected = (self.selected + 1).min(last),
            KeyCode::Char('k') | KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Char('g') | KeyCode::Home => self.selected = 0,
            KeyCode::Char('G') | KeyCode::End => self.selected = last,
            KeyCode::Enter | KeyCode::Char('l') => return self.activate(),
            KeyCode::Char('n') => return vec![Action::NewTab],
            KeyCode::Char('r') => self.refresh(),
            _ => {}
        }
        vec![]
    }

    fn wants_raw_input(&self) -> bool {
        false
    }

    fn pane_type(&self) -> PaneType {
        PaneType::Sessions
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
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> AppEvent {
        AppEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn session(id: &str, title: &str) -> SessionInfo {
        SessionInfo {
            id: id.to_string(),
            title: title.to_string(),
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    fn open(tab_index: usize, session_id: Option<&str>) -> OpenSession {
        OpenSession {
            tab_index,
            name: format!("tab {tab_index}"),
            session_id: session_id.map(str::to_string),
            status: Some(AgentStatus::Idle),
            active: tab_index == 0,
        }
    }

    fn pane() -> SessionListPane {
        let mut pane = SessionListPane::with_dir(PaneId(1), None);
        pane.history = vec![session("a", "A"), session("b", "B"), session("c", "C")];
        pane.set_open_sessions(vec![open(0, Some("b")), open(1, None)]);
        pane
    }

    #[test]
    fn open_sessions_are_hidden_from_history() {
        let pane = pane();
        assert_eq!(
            pane.entries(),
            vec![
                Entry::Open(0),
                Entry::Open(1),
                Entry::History(0),
                Entry::History(2)
            ]
        );
    }

    #[test]
    fn enter_on_open_tab_selects_tab() {
        let mut pane = pane();
        pane.handle_event(&key(KeyCode::Char('j')));
        let actions = pane.handle_event(&key(KeyCode::Enter));
        assert!(matches!(actions.as_slice(), [Action::SelectTab(1)]));
    }

    #[test]
    fn enter_on_history_resumes_session() {
        let mut pane = pane();
        pane.handle_event(&key(KeyCode::Char('G')));
        let actions = pane.handle_event(&key(KeyCode::Enter));
        assert!(matches!(
            actions.as_slice(),
            [Action::ResumeSession { id, title }] if id == "c" && title == "C"
        ));
    }

    #[test]
    fn selection_is_clamped_when_entries_shrink() {
        let mut pane = pane();
        pane.handle_event(&key(KeyCode::Char('G')));
        pane.set_open_sessions(vec![]);
        pane.history.clear();
        pane.clamp_selection();
        assert_eq!(pane.selected, 0);
    }

    #[test]
    fn relative_time_formats() {
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000_000);
        let ago = |s| base - Duration::from_secs(s);
        assert_eq!(relative_time(ago(5), base), "now");
        assert_eq!(relative_time(ago(300), base), "5m");
        assert_eq!(relative_time(ago(7200), base), "2h");
        assert_eq!(relative_time(ago(3 * 86400), base), "3d");
        assert_eq!(relative_time(ago(14 * 86400), base), "2w");
    }
}
