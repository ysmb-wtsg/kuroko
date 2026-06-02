//! カスタムキーバインドの登録・検索を管理するモジュール。
//! Luaから登録されたキーマップをApp側の入力処理から参照する。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mlua::RegistryKey;

use aijin_core::Mode;

/// キーマップの登録エントリ。
/// Luaコールバックへの参照を保持する。
pub struct KeymapEntry {
    /// Lua関数へのレジストリキー
    pub callback: RegistryKey,
}

/// モードとキー文字列からキーマップエントリを引くレジストリ。
/// Arc<Mutex<>> で LuaRuntime と App の間で共有する。
pub struct KeymapRegistry {
    /// Normal モードのキーバインド
    normal: HashMap<String, KeymapEntry>,
    /// Insert モードのキーバインド
    insert: HashMap<String, KeymapEntry>,
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
            normal: HashMap::new(),
            insert: HashMap::new(),
        }
    }

    /// キーバインドを登録する。
    ///
    /// @param mode - モード文字列（"n" = Normal, "i" = Insert）
    /// @param key - キー文字列（例: "q", "<C-a>", "<leader>f"）
    /// @param callback - Lua関数のレジストリキー
    pub fn set(&mut self, mode: &str, key: String, callback: RegistryKey) {
        let entry = KeymapEntry { callback };
        match mode {
            "n" => { self.normal.insert(key, entry); }
            "i" => { self.insert.insert(key, entry); }
            _ => {}
        }
    }

    /// 指定モード・キーに対応するエントリを検索する。
    ///
    /// @param mode - 検索対象のモード
    /// @param key - 検索対象のキー文字列
    /// @returns 登録済みならKeymapEntryの参照
    pub fn get(&self, mode: Mode, key: &str) -> Option<&KeymapEntry> {
        match mode {
            Mode::Normal => self.normal.get(key),
            Mode::Insert => self.insert.get(key),
            Mode::Select => None,
        }
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

}
