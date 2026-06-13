//! セッション状態の保存・復元を担当するモジュール。
//! サイドパネル表示状態や比率を ~/.config/krk/session.json に永続化する。

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// セッション状態。起動時に復元し、終了時に保存する。
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionState {
    /// サイドパネルに表示中の内容（"files" / "git" / null）
    #[serde(default)]
    pub side_content: Option<String>,
    /// サイドパネルの幅比率
    #[serde(default = "default_side_ratio")]
    pub side_ratio: f32,
    /// ボトムターミナルの表示状態
    #[serde(default)]
    pub bottom_visible: bool,
    /// ボトムターミナルの高さ比率（メイン側の割合）
    #[serde(default = "default_bottom_ratio")]
    pub bottom_ratio: f32,
}

/// side_ratio のデフォルト値
fn default_side_ratio() -> f32 {
    0.3
}

/// bottom_ratio のデフォルト値
fn default_bottom_ratio() -> f32 {
    0.7
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            side_content: None,
            side_ratio: default_side_ratio(),
            bottom_visible: false,
            bottom_ratio: default_bottom_ratio(),
        }
    }
}

/// セッションファイルのパスを返す
fn session_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    PathBuf::from(home).join(".config/krk/session.json")
}

/// セッション状態をファイルから読み込む。
/// ファイルが存在しない場合やパースに失敗した場合はデフォルト値を返す。
pub fn load() -> SessionState {
    // ユニットテストでは実機の ~/.config/krk/session.json を読まない
    // （開発機のセッション状態でApp::new()の初期状態が変わり、テストが不安定になるため）
    if cfg!(test) {
        return SessionState::default();
    }
    let path = session_path();
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// セッション状態をファイルに保存する。
/// ディレクトリが存在しない場合は作成する。保存失敗は無視する。
pub fn save(state: &SessionState) {
    let path = session_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(&path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_session_state() {
        let state = SessionState::default();
        assert!(state.side_content.is_none());
        assert!((state.side_ratio - 0.3).abs() < f32::EPSILON);
        assert!(!state.bottom_visible);
        assert!((state.bottom_ratio - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let state = SessionState {
            side_content: Some("git".to_string()),
            side_ratio: 0.4,
            bottom_visible: true,
            bottom_ratio: 0.6,
        };
        let json = serde_json::to_string(&state).unwrap();
        let restored: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.side_content.as_deref(), Some("git"));
        assert!((restored.side_ratio - 0.4).abs() < f32::EPSILON);
        assert!(restored.bottom_visible);
        assert!((restored.bottom_ratio - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn deserialize_old_format_falls_back_to_default() {
        // 旧フォーマットのJSONはデフォルトにフォールバック
        let old_json = r#"{"file_tree_visible":true,"left_ratio":0.2}"#;
        let state: SessionState = serde_json::from_str(old_json).unwrap();
        assert!(state.side_content.is_none());
        assert!((state.side_ratio - 0.3).abs() < f32::EPSILON);
    }
}
