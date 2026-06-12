//! kuroko-lua: Luaプラグインホスト。
//! mlua経由でLua 5.4を組み込み、krk.* 名前空間のAPIを提供する。

pub mod keymap;
mod runtime;

pub use keymap::{SharedKeymapRegistry, new_shared_registry, normalize_key_string};
pub use runtime::LuaRuntime;
