//! App構造体のファイル操作ロジック。
//! ファイルプロンプトの実行、ファイルツリーのリフレッシュ、クリップボード操作、
//! エージェントへのファイル送信を担当する。

use std::fs;
use std::path::PathBuf;

use kuroko_core::FilePromptKind;
use kuroko_filetree::FileTreePane;

use super::App;
use super::overlay::{FilePrompt, MessageLevel};

impl App {
    /// ファイル操作プロンプトの入力を実行する。
    /// FS操作を行い、成功時にツリーをリフレッシュする。
    pub(super) fn execute_file_prompt(&mut self, prompt: FilePrompt) {
        match prompt.kind {
            FilePromptKind::Create { parent_dir } => {
                let name = prompt.input.trim();
                if name.is_empty() {
                    return;
                }
                let target = parent_dir.join(name);
                let result = if name.ends_with('/') {
                    let dir_name = name.trim_end_matches('/');
                    fs::create_dir_all(parent_dir.join(dir_name))
                } else {
                    // 親ディレクトリが存在しない場合は作成
                    if let Some(parent) = target.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    fs::File::create(&target).map(|_| ())
                };
                match result {
                    Ok(()) => {
                        self.refresh_file_tree(&parent_dir);
                        self.overlay.set_status_message(format!("Created: {name}"));
                    }
                    Err(e) => {
                        self.overlay.set_status_message_with_level(
                            format!("Error: {e}"),
                            MessageLevel::Error,
                        );
                    }
                }
            }
            FilePromptKind::Rename { path, .. } => {
                let new_name = prompt.input.trim();
                if new_name.is_empty() {
                    return;
                }
                let new_path = path
                    .parent()
                    .map(|p| p.join(new_name))
                    .unwrap_or_else(|| PathBuf::from(new_name));
                match fs::rename(&path, &new_path) {
                    Ok(()) => {
                        if let Some(parent) = path.parent() {
                            self.refresh_file_tree(parent);
                        }
                        self.overlay
                            .set_status_message(format!("Renamed: {new_name}"));
                    }
                    Err(e) => {
                        self.overlay.set_status_message_with_level(
                            format!("Error: {e}"),
                            MessageLevel::Error,
                        );
                    }
                }
            }
            FilePromptKind::Delete { paths } => {
                let count = paths.len();
                let mut errors = 0;
                for path in &paths {
                    if trash::delete(path).is_err() {
                        errors += 1;
                    }
                }
                if let Some(first) = paths.first()
                    && let Some(parent) = first.parent()
                {
                    self.refresh_file_tree(parent);
                }
                if errors == 0 {
                    self.overlay
                        .set_status_message(format!("Deleted {count} item(s)"));
                } else {
                    self.overlay.set_status_message_with_level(
                        format!("Deleted with {errors} error(s)"),
                        MessageLevel::Error,
                    );
                }
            }
        }
    }

    /// ファイルツリーの指定パスを含むサブツリーをリフレッシュする。
    pub(super) fn refresh_file_tree(&mut self, target: &std::path::Path) {
        if let Some(ft_id) = self.file_tree_id
            && let Some(pane) = self.panes.get_mut(&ft_id)
            && let Some(ft) = pane.as_any_mut().downcast_mut::<FileTreePane>()
        {
            ft.refresh(target);
        }
    }

    /// ファイルパスをアクティブなエージェントタブのPTYに送り、フォーカスを移す。
    /// ファイルツリーで選択したファイルをエージェントに渡す導線として機能する。
    ///
    /// @param path - エージェントに送るファイルパス
    pub(super) fn send_file_to_agent(&mut self, path: PathBuf) {
        let pane_id = self.main_active_id();
        // プロジェクトルート（cwd）からの相対パスに変換する
        let rel = std::env::current_dir()
            .ok()
            .and_then(|cwd| path.strip_prefix(&cwd).ok().map(|p| p.to_path_buf()))
            .unwrap_or(path);
        let path_str = rel.to_string_lossy();
        // @記法でファイルをコンテキストに載せ、後続に指示を追記できるようスペースを付ける
        let data = format!("@{path_str} ").into_bytes();

        if let Some(pane) = self.panes.get_mut(&pane_id) {
            pane.write_to_pty(&data);
        }

        // エージェントタブにフォーカスを移す
        self.focused = pane_id;
    }

    /// テキストをシステムクリップボードにコピーする。
    /// 成功/失敗をステータスバーに表示する。
    pub(super) fn copy_to_clipboard(&mut self, text: &str) {
        match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text.to_string())) {
            Ok(()) => self.overlay.set_status_message("Copied!".to_string()),
            Err(e) => self
                .overlay
                .set_status_message_with_level(format!("Copy failed: {e}"), MessageLevel::Error),
        }
    }

    /// FileTreeペインから選択中パスリストを取得するヘルパー
    pub(super) fn get_filetree_selected_paths(&self) -> Vec<PathBuf> {
        self.panes
            .get(&self.focused)
            .and_then(|p| p.as_any().downcast_ref::<FileTreePane>())
            .map(|ft| ft.selected_paths())
            .unwrap_or_default()
    }
}
