//! タブの一覧管理（メインタブ・ボトムタブ共通）。
//! タブの追加・削除・切り替え・リネームを統一的に扱う。

use aijin_core::PaneId;

/// タブの一覧とアクティブ状態を管理する構造体。
/// メインペインタブとボトムターミナルタブの両方で使用する。
pub struct TabManager {
    /// タブのペインIDリスト（表示順）
    tabs: Vec<PaneId>,
    /// アクティブタブのインデックス
    active: usize,
    /// タブのカスタム名（Noneならペインのtitle()を使用）
    names: Vec<Option<String>>,
}

impl TabManager {
    /// 空のTabManagerを生成する
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            names: Vec::new(),
        }
    }

    /// 初期タブを1つ持つTabManagerを生成する
    ///
    /// @param id - 初期タブのペインID
    /// @returns TabManagerインスタンス
    pub fn with_initial(id: PaneId) -> Self {
        Self {
            tabs: vec![id],
            active: 0,
            names: vec![None],
        }
    }

    /// タブを末尾に追加し、アクティブにする
    ///
    /// @param id - 追加するタブのペインID
    pub fn add(&mut self, id: PaneId) {
        self.tabs.push(id);
        self.names.push(None);
        self.active = self.tabs.len() - 1;
    }

    /// アクティブなタブを削除し、そのPaneIdを返す。
    /// アクティブインデックスは自動的に調整される。
    ///
    /// @returns 削除されたタブのPaneId（空の場合はNone）
    pub fn remove_active(&mut self) -> Option<PaneId> {
        if self.tabs.is_empty() {
            return None;
        }
        let removed = self.tabs.remove(self.active);
        self.names.remove(self.active);
        if self.tabs.is_empty() {
            self.active = 0;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        Some(removed)
    }

    /// 次のタブに切り替える（循環）
    pub fn next(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        self.active = (self.active + 1) % self.tabs.len();
    }

    /// 前のタブに切り替える（循環）
    pub fn prev(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        self.active = if self.active == 0 {
            self.tabs.len() - 1
        } else {
            self.active - 1
        };
    }

    /// インデックス指定でタブを選択する
    ///
    /// @param index - 選択するタブのインデックス（0始まり）
    pub fn select(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    /// アクティブタブのPaneIdを返す
    ///
    /// @returns アクティブタブのPaneId（空の場合はNone）
    pub fn active_id(&self) -> Option<PaneId> {
        self.tabs.get(self.active).copied()
    }

    /// アクティブタブをリネームする。
    /// 空文字列の場合はカスタム名をクリアし、ペインのデフォルトtitleに戻る。
    ///
    /// @param name - 新しいタブ名
    pub fn rename_active(&mut self, name: String) {
        if self.active < self.names.len() {
            self.names[self.active] = if name.is_empty() { None } else { Some(name) };
        }
    }

    /// タブ数を返す
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    /// タブが空かどうかを返す
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// 指定PaneIdのタブが含まれているかを返す
    ///
    /// @param id - 検索するペインID
    /// @returns 含まれていればtrue
    pub fn contains(&self, id: &PaneId) -> bool {
        self.tabs.contains(id)
    }

    /// アクティブタブのインデックスを返す
    pub fn active_index(&self) -> usize {
        self.active
    }

    /// タブのPaneIDリストへの参照を返す
    pub fn tabs(&self) -> &[PaneId] {
        &self.tabs
    }

    /// タブ名リストへの参照を返す
    pub fn names(&self) -> &[Option<String>] {
        &self.names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- new / with_initial ---

    #[test]
    fn new_creates_empty_manager() {
        let mgr = TabManager::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
        assert_eq!(mgr.active_id(), None);
    }

    #[test]
    fn with_initial_creates_single_tab() {
        let mgr = TabManager::with_initial(PaneId(1));
        assert_eq!(mgr.len(), 1);
        assert_eq!(mgr.active_id(), Some(PaneId(1)));
        assert_eq!(mgr.active_index(), 0);
    }

    // --- add ---

    #[test]
    fn add_appends_and_activates() {
        let mut mgr = TabManager::with_initial(PaneId(1));
        mgr.add(PaneId(2));
        assert_eq!(mgr.len(), 2);
        assert_eq!(mgr.active_id(), Some(PaneId(2)));
        assert_eq!(mgr.active_index(), 1);
    }

    #[test]
    fn add_multiple_tabs() {
        let mut mgr = TabManager::new();
        mgr.add(PaneId(10));
        mgr.add(PaneId(20));
        mgr.add(PaneId(30));
        assert_eq!(mgr.len(), 3);
        assert_eq!(mgr.active_id(), Some(PaneId(30)));
        assert_eq!(mgr.tabs(), &[PaneId(10), PaneId(20), PaneId(30)]);
    }

    // --- remove_active ---

    #[test]
    fn remove_active_returns_removed_id() {
        let mut mgr = TabManager::with_initial(PaneId(1));
        mgr.add(PaneId(2));
        mgr.select(0);
        let removed = mgr.remove_active();
        assert_eq!(removed, Some(PaneId(1)));
        assert_eq!(mgr.len(), 1);
        assert_eq!(mgr.active_id(), Some(PaneId(2)));
    }

    #[test]
    fn remove_active_from_empty_returns_none() {
        let mut mgr = TabManager::new();
        assert_eq!(mgr.remove_active(), None);
    }

    #[test]
    fn remove_last_tab_makes_empty() {
        let mut mgr = TabManager::with_initial(PaneId(1));
        mgr.remove_active();
        assert!(mgr.is_empty());
        assert_eq!(mgr.active_id(), None);
    }

    #[test]
    fn remove_active_adjusts_index_when_at_end() {
        let mut mgr = TabManager::new();
        mgr.add(PaneId(1));
        mgr.add(PaneId(2));
        mgr.add(PaneId(3));
        // active = 2 (PaneId(3))
        mgr.remove_active();
        // active should be clamped to len-1 = 1
        assert_eq!(mgr.active_index(), 1);
        assert_eq!(mgr.active_id(), Some(PaneId(2)));
    }

    // --- next / prev ---

    #[test]
    fn next_cycles_forward() {
        let mut mgr = TabManager::new();
        mgr.add(PaneId(1));
        mgr.add(PaneId(2));
        mgr.add(PaneId(3));
        mgr.select(0);

        mgr.next();
        assert_eq!(mgr.active_index(), 1);
        mgr.next();
        assert_eq!(mgr.active_index(), 2);
        // 循環
        mgr.next();
        assert_eq!(mgr.active_index(), 0);
    }

    #[test]
    fn prev_cycles_backward() {
        let mut mgr = TabManager::new();
        mgr.add(PaneId(1));
        mgr.add(PaneId(2));
        mgr.add(PaneId(3));
        mgr.select(0);

        // 循環: 0 -> 2
        mgr.prev();
        assert_eq!(mgr.active_index(), 2);
        mgr.prev();
        assert_eq!(mgr.active_index(), 1);
    }

    #[test]
    fn next_single_tab_is_noop() {
        let mut mgr = TabManager::with_initial(PaneId(1));
        mgr.next();
        assert_eq!(mgr.active_index(), 0);
    }

    #[test]
    fn prev_single_tab_is_noop() {
        let mut mgr = TabManager::with_initial(PaneId(1));
        mgr.prev();
        assert_eq!(mgr.active_index(), 0);
    }

    // --- select ---

    #[test]
    fn select_valid_index() {
        let mut mgr = TabManager::new();
        mgr.add(PaneId(1));
        mgr.add(PaneId(2));
        mgr.add(PaneId(3));
        mgr.select(1);
        assert_eq!(mgr.active_id(), Some(PaneId(2)));
    }

    #[test]
    fn select_out_of_bounds_is_noop() {
        let mut mgr = TabManager::with_initial(PaneId(1));
        mgr.select(99);
        assert_eq!(mgr.active_index(), 0);
    }

    // --- rename_active ---

    #[test]
    fn rename_active_sets_name() {
        let mut mgr = TabManager::with_initial(PaneId(1));
        mgr.rename_active("my tab".to_string());
        assert_eq!(mgr.names()[0], Some("my tab".to_string()));
    }

    #[test]
    fn rename_active_empty_clears_name() {
        let mut mgr = TabManager::with_initial(PaneId(1));
        mgr.rename_active("my tab".to_string());
        mgr.rename_active("".to_string());
        assert_eq!(mgr.names()[0], None);
    }

    // --- contains ---

    #[test]
    fn contains_finds_existing_tab() {
        let mut mgr = TabManager::new();
        mgr.add(PaneId(1));
        mgr.add(PaneId(2));
        assert!(mgr.contains(&PaneId(1)));
        assert!(mgr.contains(&PaneId(2)));
        assert!(!mgr.contains(&PaneId(99)));
    }
}
