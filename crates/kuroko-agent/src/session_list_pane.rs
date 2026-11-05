//! セッション一覧サイドバー（垂直タブ）。
//! 上段に開いているメインタブ、下段に Claude Code の過去セッションを並べ、
//! 選択してEnterでタブ切り替え・セッション再開を行う。

use std::any::Any;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListItem, ListState};

use kuroko_core::theme;
use kuroko_core::{Action, AppEvent, Pane, PaneId, PaneType};

use crate::claude_sessions::{self, SessionInfo, TitleCache};
use crate::session_pins::PinStore;
use crate::status::AgentStatus;

/// 履歴を自動で読み直す間隔（表示中のみ）
const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

/// 入力が止まってから会話内容の検索を始めるまでの待ち時間
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(200);

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
    /// 新規セッションを立ち上げる行（TABS の末尾）
    New,
    /// ピン留め済みの過去セッション（`history` のインデックス）
    Pinned(usize),
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
    /// ピン留め状態
    pins: PinStore,
    /// ピン留めのプロジェクトキー（エンコード済みcwd）
    project_key: String,
    /// 検索クエリ（空なら絞り込まない）
    query: String,
    /// 検索クエリを入力中か（ペイン内部の状態。グローバルな状態は持たない）
    editing: bool,
    /// 最後にクエリを編集した時刻（会話内容検索のデバウンス用）
    last_edit: Option<Instant>,
    /// 会話内容検索の結果（セッションID → 抜粋）
    content_hits: HashMap<String, String>,
    /// `content_hits` がどのクエリの結果か
    searched_query: Option<String>,
    /// 実行中の会話内容検索（検索したクエリと結果の受信口）
    searching: Option<mpsc::Receiver<(String, HashMap<String, String>)>>,
}

impl SessionListPane {
    /// 作業ディレクトリに対応するセッション一覧ペインを生成する
    ///
    /// @param id - ペインID
    /// @param cwd - 作業ディレクトリ（Claude Code のプロジェクト識別に使う）
    pub fn new(id: PaneId, cwd: PathBuf) -> Self {
        Self::with_dir(
            id,
            claude_sessions::project_sessions_dir(&cwd),
            claude_sessions::encode_project_path(&cwd),
        )
    }

    /// セッション保存先ディレクトリとプロジェクトキーを直接指定して生成する
    fn with_dir(id: PaneId, dir: Option<PathBuf>, project_key: String) -> Self {
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
            pins: PinStore::load(),
            project_key,
            query: String::new(),
            editing: false,
            last_edit: None,
            content_hits: HashMap::new(),
            searched_query: None,
            searching: None,
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
        self.poll_search();
    }

    /// 前後の空白を除いた小文字のクエリ
    fn needle(&self) -> String {
        self.query.trim().to_lowercase()
    }

    /// 会話内容検索の結果を取り込み、クエリが変わっていれば新しい検索を始める
    fn poll_search(&mut self) {
        if let Some(rx) = &self.searching {
            match rx.try_recv() {
                Ok((query, hits)) => {
                    self.searching = None;
                    // 検索中にクエリが変わった結果は捨てる（次の呼び出しで再検索される）
                    if query == self.needle() {
                        self.content_hits = hits;
                        self.searched_query = Some(query);
                        self.clamp_selection();
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => self.searching = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let needle = self.needle();
        let settled = self
            .last_edit
            .is_none_or(|t| t.elapsed() >= SEARCH_DEBOUNCE);
        if needle.is_empty()
            || self.searching.is_some()
            || self.searched_query.as_deref() == Some(needle.as_str())
            || !settled
        {
            return;
        }
        let Some(dir) = self.dir.clone() else {
            return;
        };
        // タブで開いているセッションは過去セッション側に出ないため、検索対象から外す
        let ids: Vec<String> = self
            .history
            .iter()
            .filter(|s| !self.is_open(&s.id))
            .map(|s| s.id.clone())
            .collect();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let hits =
                claude_sessions::search_content(&dir, ids.iter().map(String::as_str), &needle);
            let _ = tx.send((needle, hits));
        });
        self.searching = Some(rx);
    }

    /// 指定セッションがタブで開かれているか
    fn is_open(&self, id: &str) -> bool {
        self.open
            .iter()
            .any(|o| o.session_id.as_deref() == Some(id))
    }

    /// 検索クエリに一致するか（タイトル、または会話内容の検索結果）。クエリが空なら常に一致
    fn matches_query(&self, session: &SessionInfo) -> bool {
        let needle = self.needle();
        if needle.is_empty() {
            return true;
        }
        let content_hit = self.searched_query.as_deref() == Some(needle.as_str())
            && self.content_hits.contains_key(&session.id);
        content_hit || session.title.to_lowercase().contains(&needle)
    }

    /// 表示中の過去セッション（タブで開いているものは除く）のインデックス。
    /// `pinned` でピン留め済み／未ピンを切り分け、検索クエリで絞り込む
    fn listed(&self, pinned: bool) -> impl Iterator<Item = usize> + '_ {
        self.history.iter().enumerate().filter_map(move |(i, s)| {
            let show = !self.is_open(&s.id)
                && self.pins.is_pinned(&self.project_key, &s.id) == pinned
                && self.matches_query(s);
            show.then_some(i)
        })
    }

    /// 選択可能な行を表示順に返す
    fn entries(&self) -> Vec<Entry> {
        (0..self.open.len())
            .map(Entry::Open)
            .chain(std::iter::once(Entry::New))
            .chain(self.listed(true).map(Entry::Pinned))
            .chain(self.listed(false).map(Entry::History))
            .collect()
    }

    fn clamp_selection(&mut self) {
        let len = self.entries().len();
        self.selected = self.selected.min(len.saturating_sub(1));
    }

    /// 行に対応するセッションID（New 行・ID不明のタブはNone）
    fn session_id_of(&self, entry: &Entry) -> Option<String> {
        match entry {
            Entry::Open(i) => self.open[*i].session_id.clone(),
            Entry::Pinned(i) | Entry::History(i) => Some(self.history[*i].id.clone()),
            Entry::New => None,
        }
    }

    /// 選択行のセッションのピン留めを切り替える。
    /// 行は PINNED / HISTORY の間を移るため、選択も同じセッションに追従させる
    fn toggle_pin(&mut self) {
        let entries = self.entries();
        let Some(id) = entries
            .get(self.selected)
            .and_then(|e| self.session_id_of(e))
        else {
            return;
        };
        self.pins.toggle(&self.project_key, &id);
        let moved = self
            .entries()
            .iter()
            .position(|e| self.session_id_of(e).as_deref() == Some(id.as_str()));
        if let Some(index) = moved {
            self.selected = index;
        }
        self.clamp_selection();
    }

    /// 検索結果の先頭（PINNED / HISTORY の最初の行）を選択する。無ければ選択は変えない
    fn select_first_hit(&mut self) {
        if let Some(index) = self
            .entries()
            .iter()
            .position(|e| matches!(e, Entry::Pinned(_) | Entry::History(_)))
        {
            self.selected = index;
        }
    }

    /// 検索を解除して絞り込みを戻す
    fn clear_search(&mut self) {
        self.query.clear();
        self.editing = false;
        self.last_edit = None;
        self.content_hits.clear();
        self.searched_query = None;
        self.clamp_selection();
    }

    /// 検索クエリ入力中のキー処理。文字はすべてクエリに入る
    fn handle_search_key(&mut self, key: &ratatui::crossterm::event::KeyEvent) {
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.query.push(c);
            }
            KeyCode::Backspace => {
                self.query.pop();
            }
            KeyCode::Enter => {
                self.editing = false;
                return;
            }
            KeyCode::Esc => {
                self.clear_search();
                return;
            }
            _ => return,
        }
        self.last_edit = Some(Instant::now());
        self.select_first_hit();
        self.clamp_selection();
    }

    /// 選択行に対するEnterの動作。
    /// タブを開いたらサイドバーは役目を終えるので、併せて閉じる。
    fn activate(&self) -> Vec<Action> {
        let action = match self.entries().get(self.selected) {
            Some(Entry::Open(i)) => Action::SelectTab(self.open[*i].tab_index),
            Some(Entry::New) => Action::NewTab,
            Some(Entry::Pinned(i) | Entry::History(i)) => {
                let session = &self.history[*i];
                Action::ResumeSession {
                    id: session.id.clone(),
                    title: session.title.clone(),
                }
            }
            None => return vec![],
        };
        open_and_close(action)
    }
}

/// タブを開くアクションの後にサイドバーを閉じるアクションを続ける。
/// サイドバーのペインがイベントを受けるのは表示中のみなので、トグルで閉じられる。
fn open_and_close(action: Action) -> Vec<Action> {
    vec![action, Action::ToggleSessions]
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
        let mut inner = area.inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        // 検索中は最上段に検索バーを出し、残りを一覧にする
        if self.editing || !self.query.is_empty() {
            let bar = Rect { height: 1, ..inner };
            let cursor = if self.editing { "▏" } else { "" };
            frame.render_widget(
                Line::from(vec![
                    Span::styled("/ ", Style::default().fg(t.accent_secondary)),
                    Span::styled(
                        format!("{}{cursor}", self.query),
                        Style::default().fg(t.text_primary),
                    ),
                ]),
                bar,
            );
            inner.y += 1;
            inner.height = inner.height.saturating_sub(1);
        }
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
            let prev = n.checked_sub(1).and_then(|p| entries.get(p));
            let heading = match entry {
                Entry::Pinned(_) if !matches!(prev, Some(Entry::Pinned(_))) => Some("PINNED"),
                Entry::History(_) if !matches!(prev, Some(Entry::History(_))) => Some("HISTORY"),
                _ => None,
            };
            if let Some(heading) = heading {
                items.push(ListItem::new(Line::raw("")));
                items.push(ListItem::new(Line::styled(heading, header_style)));
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
                    let mut spans = vec![
                        Span::styled(format!("{mark} "), Style::default().fg(color)),
                        Span::styled(open.name.clone(), name_style),
                    ];
                    let pinned = open
                        .session_id
                        .as_deref()
                        .is_some_and(|id| self.pins.is_pinned(&self.project_key, id));
                    if pinned {
                        spans.push(Span::styled(" ★", Style::default().fg(t.accent_warning)));
                    }
                    Line::from(spans)
                }
                Entry::New => Line::styled("+ New session", Style::default().fg(t.text_muted)),
                Entry::Pinned(i) | Entry::History(i) => {
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
            // 会話内容でヒットした行は、タイトルの下に一致箇所の抜粋を薄色で添える
            let snippet = match entry {
                Entry::Pinned(i) | Entry::History(i) => self
                    .searched_query
                    .as_ref()
                    .and_then(|_| self.content_hits.get(&self.history[*i].id)),
                _ => None,
            };
            if let Some(snippet) = snippet {
                items.push(ListItem::new(Text::from(vec![
                    line,
                    Line::styled(format!("    {snippet}"), Style::default().fg(t.text_dim)),
                ])));
                continue;
            }
            items.push(ListItem::new(line));
        }
        if !self.query.is_empty()
            && !entries
                .iter()
                .any(|e| matches!(e, Entry::Pinned(_) | Entry::History(_)))
        {
            items.push(ListItem::new(Line::styled(
                "(no matches)",
                Style::default().fg(t.text_dim),
            )));
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
        if self.editing {
            self.handle_search_key(key);
            return vec![];
        }
        let last = self.entries().len().saturating_sub(1);
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.selected = (self.selected + 1).min(last),
            KeyCode::Char('k') | KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Char('g') | KeyCode::Home => self.selected = 0,
            KeyCode::Char('G') | KeyCode::End => self.selected = last,
            KeyCode::Enter | KeyCode::Char('l') => return self.activate(),
            KeyCode::Char('n') => return open_and_close(Action::NewTab),
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Char('p') => self.toggle_pin(),
            KeyCode::Char('/') => self.editing = true,
            KeyCode::Esc if !self.query.is_empty() => self.clear_search(),
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
        let mut pane = SessionListPane::with_dir(PaneId(1), None, "proj".to_string());
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
                Entry::New,
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
        assert!(matches!(
            actions.as_slice(),
            [Action::SelectTab(1), Action::ToggleSessions]
        ));
    }

    #[test]
    fn enter_on_new_row_starts_new_session() {
        let mut pane = pane();
        pane.handle_event(&key(KeyCode::Char('j')));
        pane.handle_event(&key(KeyCode::Char('j')));
        let actions = pane.handle_event(&key(KeyCode::Enter));
        assert!(matches!(
            actions.as_slice(),
            [Action::NewTab, Action::ToggleSessions]
        ));
    }

    #[test]
    fn n_starts_new_session_and_closes_sidebar() {
        let mut pane = pane();
        let actions = pane.handle_event(&key(KeyCode::Char('n')));
        assert!(matches!(
            actions.as_slice(),
            [Action::NewTab, Action::ToggleSessions]
        ));
    }

    #[test]
    fn enter_on_history_resumes_session() {
        let mut pane = pane();
        pane.handle_event(&key(KeyCode::Char('G')));
        let actions = pane.handle_event(&key(KeyCode::Enter));
        assert!(matches!(
            actions.as_slice(),
            [Action::ResumeSession { id, title }, Action::ToggleSessions]
                if id == "c" && title == "C"
        ));
    }

    fn type_str(pane: &mut SessionListPane, text: &str) {
        for c in text.chars() {
            pane.handle_event(&key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn pinned_sessions_move_to_pinned_section() {
        let mut pane = pane();
        // Open(0), Open(1), New, History(0)=a, History(2)=c の a を選んでピン留め
        for _ in 0..3 {
            pane.handle_event(&key(KeyCode::Char('j')));
        }
        pane.handle_event(&key(KeyCode::Char('p')));
        assert_eq!(
            pane.entries(),
            vec![
                Entry::Open(0),
                Entry::Open(1),
                Entry::New,
                Entry::Pinned(0),
                Entry::History(2)
            ]
        );
        // 選択はピン留めしたセッションに追従する
        assert_eq!(pane.entries()[pane.selected], Entry::Pinned(0));

        // もう一度 p で解除
        pane.handle_event(&key(KeyCode::Char('p')));
        assert_eq!(pane.entries()[pane.selected], Entry::History(0));
    }

    #[test]
    fn pinned_open_session_stays_in_tabs_only() {
        let mut pane = pane();
        // Open(0) は session b
        pane.handle_event(&key(KeyCode::Char('p')));
        assert!(pane.pins.is_pinned("proj", "b"));
        assert!(!pane.entries().iter().any(|e| matches!(e, Entry::Pinned(_))));
    }

    #[test]
    fn pin_on_tab_without_session_id_or_new_row_is_noop() {
        let mut pane = pane();
        pane.handle_event(&key(KeyCode::Char('j')));
        pane.handle_event(&key(KeyCode::Char('p')));
        pane.handle_event(&key(KeyCode::Char('j')));
        pane.handle_event(&key(KeyCode::Char('p')));
        assert!(!pane.pins.is_pinned("proj", "a"));
    }

    #[test]
    fn enter_on_pinned_resumes_session() {
        let mut pane = pane();
        pane.pins.toggle("proj", "c");
        pane.handle_event(&key(KeyCode::Char('G')));
        // PINNED の c が History より前にあるので末尾は a
        pane.selected = pane
            .entries()
            .iter()
            .position(|e| matches!(e, Entry::Pinned(_)))
            .unwrap();
        let actions = pane.handle_event(&key(KeyCode::Enter));
        assert!(matches!(
            actions.as_slice(),
            [Action::ResumeSession { id, .. }, Action::ToggleSessions] if id == "c"
        ));
    }

    #[test]
    fn search_filters_by_title_and_selects_first_hit() {
        let mut pane = pane();
        pane.handle_event(&key(KeyCode::Char('/')));
        assert!(pane.editing);
        // 入力中は j / p もクエリの文字になる
        type_str(&mut pane, "c");
        assert_eq!(
            pane.entries(),
            vec![
                Entry::Open(0),
                Entry::Open(1),
                Entry::New,
                Entry::History(2)
            ]
        );
        assert_eq!(pane.entries()[pane.selected], Entry::History(2));

        type_str(&mut pane, "jp");
        assert_eq!(pane.query, "cjp");
        assert!(!pane.pins.is_pinned("proj", "a"));
    }

    #[test]
    fn search_is_case_insensitive_and_backspace_widens() {
        let mut pane = pane();
        pane.handle_event(&key(KeyCode::Char('/')));
        type_str(&mut pane, "Ab");
        assert!(
            !pane
                .entries()
                .iter()
                .any(|e| matches!(e, Entry::History(_)))
        );
        pane.handle_event(&key(KeyCode::Backspace));
        assert_eq!(pane.query, "A");
        assert_eq!(
            pane.entries()
                .iter()
                .filter(|e| matches!(e, Entry::History(_)))
                .count(),
            1
        );
    }

    #[test]
    fn enter_ends_editing_and_esc_clears_query() {
        let mut pane = pane();
        pane.handle_event(&key(KeyCode::Char('/')));
        type_str(&mut pane, "a");
        pane.handle_event(&key(KeyCode::Enter));
        assert!(!pane.editing);
        assert_eq!(pane.query, "a");

        // 入力終了後は通常のキー操作に戻る（Enter はタブ切り替え等を返す）
        let actions = pane.handle_event(&key(KeyCode::Enter));
        assert!(matches!(
            actions.as_slice(),
            [Action::ResumeSession { .. }, _]
        ));

        pane.handle_event(&key(KeyCode::Esc));
        assert!(pane.query.is_empty());
        assert_eq!(pane.entries().len(), 5);
    }

    #[test]
    fn esc_while_editing_clears_and_stops_editing() {
        let mut pane = pane();
        pane.handle_event(&key(KeyCode::Char('/')));
        type_str(&mut pane, "zzz");
        pane.handle_event(&key(KeyCode::Esc));
        assert!(!pane.editing);
        assert!(pane.query.is_empty());
    }

    #[test]
    fn content_search_adds_hits_beyond_title_matches() {
        let dir = std::env::temp_dir().join(format!("krk-pane-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.jsonl"),
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"the Quokka is here"}]}}"#,
        )
        .unwrap();

        let mut pane = pane();
        pane.dir = Some(dir.clone());
        pane.handle_event(&key(KeyCode::Char('/')));
        type_str(&mut pane, "quokka");
        // タイトル（A/B/C）には一致しないので、内容検索の完了までHISTORYは空
        assert!(
            !pane
                .entries()
                .iter()
                .any(|e| matches!(e, Entry::History(_)))
        );

        pane.last_edit = Some(Instant::now() - Duration::from_secs(1));
        for _ in 0..100 {
            pane.poll_search();
            if pane.searched_query.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(pane.entries().last(), Some(&Entry::History(0)));
        assert_eq!(pane.content_hits["a"], "the Quokka is here");
        std::fs::remove_dir_all(&dir).unwrap();
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
