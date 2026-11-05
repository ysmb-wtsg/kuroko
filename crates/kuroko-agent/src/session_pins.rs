//! セッションのピン留め状態の保存・復元。
//! `~/.config/krk/pinned_sessions.json` にプロジェクト（エンコード済みcwd）ごとのセッションIDを保存する。
//! トグルのたびに即保存する（終了時保存だとクラッシュで失われるため）。

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// ピン留め状態のストア
pub struct PinStore {
    /// 保存先（Noneなら保存しない）
    path: Option<PathBuf>,
    /// プロジェクトキー → ピン留め済みセッションID（ピン留めした順）
    map: BTreeMap<String, Vec<String>>,
}

/// 既定の保存先を返す（HOME不明ならNone）
fn default_path() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".config/krk/pinned_sessions.json"))
}

impl PinStore {
    /// 既定の保存先から読み込む。
    /// ユニットテストでは実機のファイルを読み書きしない（開発機の状態でテストが不安定になるため）。
    pub fn load() -> Self {
        if cfg!(test) {
            return Self::at(None);
        }
        Self::at(default_path())
    }

    /// 保存先を指定して読み込む。ファイルが無い・壊れている場合は空で始める
    fn at(path: Option<PathBuf>) -> Self {
        let map = path
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self { path, map }
    }

    /// 指定セッションがピン留めされているかを返す
    ///
    /// @param project - プロジェクトキー（エンコード済みcwd）
    /// @param id - セッションID
    pub fn is_pinned(&self, project: &str, id: &str) -> bool {
        self.map
            .get(project)
            .is_some_and(|ids| ids.iter().any(|i| i == id))
    }

    /// ピン留めを切り替えて保存する
    ///
    /// @param project - プロジェクトキー（エンコード済みcwd）
    /// @param id - セッションID
    /// @returns 切り替え後にピン留めされていればtrue
    pub fn toggle(&mut self, project: &str, id: &str) -> bool {
        let ids = self.map.entry(project.to_string()).or_default();
        let pinned = if let Some(pos) = ids.iter().position(|i| i == id) {
            ids.remove(pos);
            false
        } else {
            ids.push(id.to_string());
            true
        };
        if ids.is_empty() {
            self.map.remove(project);
        }
        self.save();
        pinned
    }

    /// 保存失敗は無視する（ピン留めが使えなくなるだけで操作は止めない）
    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.map) {
            let _ = fs::write(path, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("krk-pins-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("pinned_sessions.json")
    }

    #[test]
    fn toggle_adds_and_removes() {
        let mut store = PinStore::at(None);
        assert!(!store.is_pinned("p", "a"));
        assert!(store.toggle("p", "a"));
        assert!(store.is_pinned("p", "a"));
        assert!(!store.toggle("p", "a"));
        assert!(!store.is_pinned("p", "a"));
    }

    #[test]
    fn pins_are_separated_by_project() {
        let mut store = PinStore::at(None);
        store.toggle("p1", "a");
        assert!(store.is_pinned("p1", "a"));
        assert!(!store.is_pinned("p2", "a"));
    }

    #[test]
    fn toggle_persists_immediately_and_roundtrips() {
        let path = temp_path("roundtrip");
        let mut store = PinStore::at(Some(path.clone()));
        store.toggle("p", "a");
        store.toggle("p", "b");
        store.toggle("p", "a");

        let restored = PinStore::at(Some(path.clone()));
        assert!(!restored.is_pinned("p", "a"));
        assert!(restored.is_pinned("p", "b"));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn broken_file_starts_empty() {
        let path = temp_path("broken");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not json").unwrap();
        let store = PinStore::at(Some(path.clone()));
        assert!(!store.is_pinned("p", "a"));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
