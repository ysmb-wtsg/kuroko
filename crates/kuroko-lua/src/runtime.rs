//! Lua VMの初期化とkrk.* 名前空間の登録を行うモジュール。
//! Luaからの操作はmpscチャネル経由でActionとしてメインループに送信される。

use std::sync::mpsc;

use mlua::{Function, Lua, Table};

use crate::keymap::{SharedKeymapRegistry, new_shared_registry, normalize_key_string};
use kuroko_core::{Action, Direction, KurokoError};

/// Luaランタイムの管理構造体。
/// Lua VMとActionの送信チャネルとキーマップレジストリを保持する。
pub struct LuaRuntime {
    /// Lua VM
    lua: Lua,
    /// Actionの送信チャネル
    action_tx: mpsc::Sender<Action>,
    /// 共有キーマップレジストリ
    keymap_registry: SharedKeymapRegistry,
}

impl LuaRuntime {
    /// 新しいLuaRuntimeを初期化し、krk.* APIを登録する。
    ///
    /// @param action_tx - Luaから発行されたActionの送信先
    /// @returns LuaRuntimeインスタンス
    pub fn new(action_tx: mpsc::Sender<Action>) -> Result<Self, KurokoError> {
        let lua = Lua::new();
        let keymap_registry = new_shared_registry();
        let runtime = Self {
            lua,
            action_tx,
            keymap_registry,
        };
        runtime
            .register_api()
            .map_err(|e| KurokoError::Lua(e.to_string()))?;
        Ok(runtime)
    }

    /// 共有キーマップレジストリへの参照を返す。
    /// App側でキーマップ検索に使用する。
    pub fn keymap_registry(&self) -> SharedKeymapRegistry {
        self.keymap_registry.clone()
    }

    /// krk.* 名前空間のAPI関数をLuaグローバルに登録する
    fn register_api(&self) -> mlua::Result<()> {
        let globals = self.lua.globals();

        let krk = self.lua.create_table()?;

        // krk.keymap
        let keymap = self.create_keymap_module()?;
        krk.set("keymap", keymap)?;

        // krk.pane
        let pane = self.create_pane_module()?;
        krk.set("pane", pane)?;

        // krk.opt
        let opt = self.lua.create_table()?;
        opt.set("leader", " ")?;
        opt.set("tick_rate", 50)?;
        opt.set("main_pane", "claude-code")?;
        krk.set("opt", opt)?;

        globals.set("krk", krk)?;
        Ok(())
    }

    /// krk.keymap モジュールを作成する。
    /// `krk.keymap.set(context, key, callback)` でカスタムキーバインドを登録し、
    /// `krk.keymap.set_toggle_key(key)` でグローバルレイヤーのトグルキーを変更できる。
    fn create_keymap_module(&self) -> mlua::Result<Table> {
        let keymap = self.lua.create_table()?;
        let registry = self.keymap_registry.clone();

        // krk.keymap.set(context, key, callback)
        // context は "global"（グローバルレイヤー中） | "direct"（直通中の先取り）
        let set_fn = self.lua.create_function(
            move |lua, (context, key, callback): (String, String, Function)| {
                // リーダーキーの展開: krk.opt.leader を取得
                let leader = lua
                    .globals()
                    .get::<mlua::Table>("krk")
                    .and_then(|a| a.get::<mlua::Table>("opt"))
                    .and_then(|o| o.get::<String>("leader"))
                    .unwrap_or_else(|_| " ".to_string());
                let normalized_key = normalize_key_string(&key, &leader);

                // コールバックをLuaレジストリに保存
                let registry_key = lua.create_registry_value(callback).map_err(|e| {
                    mlua::Error::RuntimeError(format!("Failed to register keymap callback: {e}"))
                })?;

                // キーマップレジストリに登録
                let mut reg = registry.lock().unwrap();
                if !reg.set(&context, normalized_key, registry_key) {
                    return Err(mlua::Error::RuntimeError(format!(
                        "Unknown keymap context: {context} (expected \"global\" or \"direct\")"
                    )));
                }
                Ok(())
            },
        )?;
        keymap.set("set", set_fn)?;

        // krk.keymap.set_toggle_key(key)
        // グローバルレイヤーのトグルキーを変更する（デフォルト: <C-Space>）
        let registry = self.keymap_registry.clone();
        let set_toggle_fn = self.lua.create_function(move |_, key: String| {
            let mut reg = registry.lock().unwrap();
            reg.set_toggle_key(key);
            Ok(())
        })?;
        keymap.set("set_toggle_key", set_toggle_fn)?;

        Ok(keymap)
    }

    /// krk.pane モジュールを作成する
    fn create_pane_module(&self) -> mlua::Result<Table> {
        let pane = self.lua.create_table()?;
        let tx = self.action_tx.clone();

        // krk.pane.toggle(type)
        // パネルの表示/非表示を切り替える
        let toggle_fn = self.lua.create_function(move |_, pane_type: String| {
            let action = match pane_type.as_str() {
                "terminal" => Action::ToggleTerminal,
                "files" | "filetree" => Action::ToggleFileTree,
                _ => {
                    let _ = tx.send(Action::Notify(format!("Unknown pane type: {pane_type}")));
                    return Ok(());
                }
            };
            let _ = tx.send(action);
            Ok(())
        })?;
        pane.set("toggle", toggle_fn)?;

        let tx2 = self.action_tx.clone();
        let focus_fn = self.lua.create_function(move |_, direction: String| {
            let action = match direction.as_str() {
                "next" => Action::FocusNext,
                "prev" => Action::FocusPrev,
                "left" => Action::FocusDirection(Direction::Left),
                "right" => Action::FocusDirection(Direction::Right),
                "up" => Action::FocusDirection(Direction::Up),
                "down" => Action::FocusDirection(Direction::Down),
                _ => {
                    let _ = tx2.send(Action::Notify(format!(
                        "Unknown focus direction: {direction}"
                    )));
                    return Ok(());
                }
            };
            let _ = tx2.send(action);
            Ok(())
        })?;
        pane.set("focus", focus_fn)?;

        Ok(pane)
    }

    /// krk.opt テーブルから文字列設定値を取得する。
    ///
    /// @param key - 取得するキー名
    /// @returns 値が存在すれば文字列、なければNone
    pub fn get_opt_string(&self, key: &str) -> Option<String> {
        let globals = self.lua.globals();
        let krk: Table = globals.get("krk").ok()?;
        let opt: Table = krk.get("opt").ok()?;
        opt.get::<String>(key).ok()
    }

    /// Luaファイルを実行する。
    ///
    /// @param path - 実行するLuaファイルのパス
    pub fn exec_file(&self, path: &std::path::Path) -> Result<(), KurokoError> {
        let code = std::fs::read_to_string(path)?;
        self.lua
            .load(&code)
            .exec()
            .map_err(|e| KurokoError::Lua(e.to_string()))?;
        Ok(())
    }

    /// Luaからのアクションを受信チャネルから取り出す（メインループ用）。
    /// LuaRuntimeが内部でaction_txを保持しているため、対応するrxは呼び出し元が管理する。
    pub fn action_sender(&self) -> mpsc::Sender<Action> {
        self.action_tx.clone()
    }

    /// キーマップレジストリに登録されたLuaコールバックを実行する。
    /// コールバック内で `krk.pane.*` 等のAPI呼び出しがあれば、
    /// action_tx経由でActionが送信される。
    ///
    /// @param registry_key - 実行するLuaコールバックのレジストリキー
    /// @returns 実行成功ならOk、エラーならErr
    pub fn exec_callback(&self, registry_key: &mlua::RegistryKey) -> Result<(), KurokoError> {
        let callback: Function = self
            .lua
            .registry_value(registry_key)
            .map_err(|e| KurokoError::Lua(e.to_string()))?;
        callback
            .call::<()>(())
            .map_err(|e| KurokoError::Lua(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::KeymapContext;
    use std::sync::mpsc;

    /// テスト用のLuaRuntimeとAction受信チャネルを生成するヘルパー
    fn setup() -> (LuaRuntime, mpsc::Receiver<Action>) {
        let (tx, rx) = mpsc::channel();
        let runtime = LuaRuntime::new(tx).expect("LuaRuntime initialization should succeed");
        (runtime, rx)
    }

    #[test]
    fn new_creates_runtime() {
        let (tx, _rx) = mpsc::channel();
        let result = LuaRuntime::new(tx);
        assert!(result.is_ok());
    }

    #[test]
    fn opt_default_main_pane() {
        let (runtime, _rx) = setup();
        assert_eq!(
            runtime.get_opt_string("main_pane"),
            Some("claude-code".to_string())
        );
    }

    #[test]
    fn opt_default_leader() {
        let (runtime, _rx) = setup();
        assert_eq!(runtime.get_opt_string("leader"), Some(" ".to_string()));
    }

    #[test]
    fn opt_nonexistent_key_returns_none() {
        let (runtime, _rx) = setup();
        assert_eq!(runtime.get_opt_string("nonexistent"), None);
    }

    #[test]
    fn pane_toggle_terminal_sends_action() {
        let (runtime, rx) = setup();
        runtime
            .lua
            .load(r#"krk.pane.toggle("terminal")"#)
            .exec()
            .unwrap();
        let action = rx.try_recv().unwrap();
        assert!(matches!(action, Action::ToggleTerminal));
    }

    #[test]
    fn pane_toggle_files_sends_action() {
        let (runtime, rx) = setup();
        runtime
            .lua
            .load(r#"krk.pane.toggle("files")"#)
            .exec()
            .unwrap();
        let action = rx.try_recv().unwrap();
        assert!(matches!(action, Action::ToggleFileTree));
    }

    #[test]
    fn pane_toggle_unknown_sends_notify() {
        let (runtime, rx) = setup();
        runtime
            .lua
            .load(r#"krk.pane.toggle("unknown")"#)
            .exec()
            .unwrap();
        let action = rx.try_recv().unwrap();
        assert!(matches!(action, Action::Notify(_)));
    }

    #[test]
    fn pane_focus_sends_action() {
        let (runtime, rx) = setup();
        runtime
            .lua
            .load(r#"krk.pane.focus("left")"#)
            .exec()
            .unwrap();
        let action = rx.try_recv().unwrap();
        assert!(matches!(action, Action::FocusDirection(Direction::Left)));
    }

    #[test]
    fn keymap_set_registers_binding() {
        let (runtime, _rx) = setup();
        let result = runtime
            .lua
            .load(r#"krk.keymap.set("global", "q", function() end)"#)
            .exec();
        assert!(result.is_ok());
        // レジストリにグローバルコンテキストの "q" が登録されていることを確認
        let reg = runtime.keymap_registry.lock().unwrap();
        assert!(reg.get(KeymapContext::Global, "q").is_some());
    }

    #[test]
    fn keymap_set_registers_direct_binding() {
        let (runtime, _rx) = setup();
        runtime
            .lua
            .load(r#"krk.keymap.set("direct", "<C-h>", function() end)"#)
            .exec()
            .unwrap();
        let reg = runtime.keymap_registry.lock().unwrap();
        assert!(reg.get(KeymapContext::Direct, "<C-h>").is_some());
        assert!(reg.get(KeymapContext::Global, "<C-h>").is_none());
    }

    #[test]
    fn keymap_set_rejects_unknown_context() {
        let (runtime, _rx) = setup();
        let result = runtime
            .lua
            .load(r#"krk.keymap.set("n", "q", function() end)"#)
            .exec();
        assert!(result.is_err());
    }

    #[test]
    fn keymap_set_expands_leader() {
        let (runtime, _rx) = setup();
        // リーダーキーをカンマに変更
        runtime.lua.load(r#"krk.opt.leader = ",""#).exec().unwrap();
        runtime
            .lua
            .load(r#"krk.keymap.set("global", "<leader>f", function() end)"#)
            .exec()
            .unwrap();
        let reg = runtime.keymap_registry.lock().unwrap();
        // "<leader>f" が ",f" に展開されていることを確認
        assert!(reg.get(KeymapContext::Global, ",f").is_some());
        assert!(reg.get(KeymapContext::Global, "<leader>f").is_none());
    }

    #[test]
    fn keymap_set_toggle_key() {
        let (runtime, _rx) = setup();
        runtime
            .lua
            .load(r#"krk.keymap.set_toggle_key("<C-g>")"#)
            .exec()
            .unwrap();
        let reg = runtime.keymap_registry.lock().unwrap();
        assert_eq!(reg.toggle_key(), "<C-g>");
    }

    #[test]
    fn keymap_callback_sends_action() {
        let (runtime, rx) = setup();
        runtime
            .lua
            .load(r#"krk.keymap.set("global", "t", function() krk.pane.toggle("terminal") end)"#)
            .exec()
            .unwrap();
        // コールバックを実行する（レジストリのロックを解放してからexec_callbackを呼ぶ）
        {
            let reg = runtime.keymap_registry.lock().unwrap();
            let entry = reg.get(KeymapContext::Global, "t").unwrap();
            runtime.exec_callback(&entry.callback).unwrap();
        }
        let action = rx.try_recv().unwrap();
        assert!(matches!(action, Action::ToggleTerminal));
    }

    #[test]
    fn exec_file_nonexistent_returns_error() {
        let (runtime, _rx) = setup();
        let result = runtime.exec_file(std::path::Path::new("/nonexistent/file.lua"));
        assert!(result.is_err());
    }

    #[test]
    fn exec_file_sets_option() {
        let (runtime, _rx) = setup();
        // 一時ファイルにLuaスクリプトを書き出してexec_fileで実行する
        let dir = std::env::temp_dir();
        let path = dir.join("kuroko_test_exec.lua");
        std::fs::write(&path, r#"krk.opt.main_pane = "terminal""#).unwrap();

        let result = runtime.exec_file(&path);
        assert!(result.is_ok());
        assert_eq!(
            runtime.get_opt_string("main_pane"),
            Some("terminal".to_string())
        );

        // テスト後の一時ファイル削除
        let _ = std::fs::remove_file(&path);
    }
}
