//! カスタムキーバインドの登録・検索を管理するモジュール。
//! Luaから登録されたキーマップをApp側の入力処理から参照する。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mlua::RegistryKey;

/// グローバルレイヤーのトグルキーのデフォルト値。
/// 改行系（Enter / Ctrl+j）と無関係で全端末に確実に届き、エージェントの入力を奪わない
/// （Ctrl+Spaceは端末によりNUL不達、Ctrl+jはエージェントの改行挿入と衝突するため不採用）。
pub const DEFAULT_TOGGLE_KEY: &str = "<C-g>";

/// キーマップの検索コンテキスト。
/// Global = グローバルレイヤー中、Direct = 直通中（キーがペインへ流れる状態）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeymapContext {
    Global,
    Direct,
}

/// キーマップの登録エントリ。
/// Luaコールバックへの参照を保持する。
pub struct KeymapEntry {
    /// Lua関数へのレジストリキー
    pub callback: RegistryKey,
}

/// コンテキストとキー文字列からキーマップエントリを引くレジストリ。
/// Arc<Mutex<>> で LuaRuntime と App の間で共有する。
pub struct KeymapRegistry {
    /// グローバルレイヤー中のキーバインド
    global: HashMap<String, KeymapEntry>,
    /// 直通中にAppが先取りするキーバインド（デフォルト空＝衝突ゼロ）
    direct: HashMap<String, KeymapEntry>,
    /// グローバルレイヤーのトグルキー（Vim記法）
    toggle_key: String,
}

impl Default for KeymapRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl KeymapRegistry {
    /// 空のレジストリを生成する
    pub fn new() -> Self {
        Self {
            global: HashMap::new(),
            direct: HashMap::new(),
            toggle_key: DEFAULT_TOGGLE_KEY.to_string(),
        }
    }

    /// キーバインドを登録する。
    ///
    /// @param context - コンテキスト文字列（"global" | "direct"）
    /// @param key - キー文字列（例: "q", "<C-a>", "<leader>f"）
    /// @param callback - Lua関数のレジストリキー
    /// @returns 不明なコンテキストの場合は false
    pub fn set(&mut self, context: &str, key: String, callback: RegistryKey) -> bool {
        let entry = KeymapEntry { callback };
        match context {
            "global" => {
                self.global.insert(key, entry);
                true
            }
            "direct" => {
                self.direct.insert(key, entry);
                true
            }
            _ => false,
        }
    }

    /// 指定コンテキスト・キーに対応するエントリを検索する。
    ///
    /// @param context - 検索対象のコンテキスト
    /// @param key - 検索対象のキー文字列
    /// @returns 登録済みならKeymapEntryの参照
    pub fn get(&self, context: KeymapContext, key: &str) -> Option<&KeymapEntry> {
        match context {
            KeymapContext::Global => self.global.get(key),
            KeymapContext::Direct => self.direct.get(key),
        }
    }

    /// グローバルレイヤーのトグルキーを返す
    pub fn toggle_key(&self) -> &str {
        &self.toggle_key
    }

    /// グローバルレイヤーのトグルキーを変更する
    pub fn set_toggle_key(&mut self, key: String) {
        self.toggle_key = key;
    }
}

/// スレッド安全な共有キーマップレジストリ
pub type SharedKeymapRegistry = Arc<Mutex<KeymapRegistry>>;

/// 新しい共有キーマップレジストリを生成する
pub fn new_shared_registry() -> SharedKeymapRegistry {
    Arc::new(Mutex::new(KeymapRegistry::new()))
}

/// Lua側で登録されたキー文字列（Vim記法）をパースして正規化する。
/// `<leader>` プレフィックスをリーダーキーの値に展開する。
///
/// @param key_str - Lua側で指定されたキー文字列
/// @param leader - リーダーキーの値
/// @returns 正規化されたキー文字列
pub fn normalize_key_string(key_str: &str, leader: &str) -> String {
    if let Some(rest) = key_str.strip_prefix("<leader>") {
        format!("{leader}{rest}")
    } else {
        key_str.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_leader_prefix() {
        assert_eq!(normalize_key_string("<leader>f", " "), " f");
        assert_eq!(normalize_key_string("<leader>t", ","), ",t");
    }

    #[test]
    fn normalize_plain_key() {
        assert_eq!(normalize_key_string("q", " "), "q");
        assert_eq!(normalize_key_string("<C-a>", " "), "<C-a>");
    }

    #[test]
    fn default_toggle_key() {
        let reg = KeymapRegistry::new();
        assert_eq!(reg.toggle_key(), "<C-g>");
    }

    #[test]
    fn set_toggle_key_overrides_default() {
        let mut reg = KeymapRegistry::new();
        reg.set_toggle_key("<C-g>".to_string());
        assert_eq!(reg.toggle_key(), "<C-g>");
    }
}
