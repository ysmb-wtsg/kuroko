//! オーバーレイUI（コマンドパレット/プレビュー/プロンプト/情報/メッセージ/リネーム）の状態管理。
//! App構造体からオーバーレイ関連の状態とデータ型を分離する。

use std::fs;
use std::path::PathBuf;

use ratatui::style::{Color, Style};

use kuroko_core::{Action, FilePromptKind};
use kuroko_core::theme;

/// プレビュー表示の最大読み込み行数
pub const PREVIEW_MAX_LINES: usize = 500;

/// オーバーレイUIの集約状態。
/// ファイルプレビュー、操作プロンプト、詳細情報、一時メッセージ、タブリネームの状態を保持する。
pub struct OverlayState {
    /// コマンドパレットの状態（Noneなら非表示）
    pub command_palette: Option<CommandPalette>,
    /// ファイルプレビューの状態（Noneならプレビュー非表示）
    pub file_preview: Option<FilePreview>,
    /// ファイル操作プロンプトの状態（Noneならプロンプト非表示）
    pub file_prompt: Option<FilePrompt>,
    /// ファイル詳細情報の状態（Noneなら非表示）
    pub file_info: Option<FileInfo>,
    /// ステータスバーの一時メッセージ
    pub status_message: Option<StatusMessage>,
    /// タブリネーム中の入力文字列（Noneならリネームモードでない）
    pub rename_input: Option<String>,
    /// ヘルプオーバーレイの表示状態
    pub help_visible: bool,
    /// リネーム対象がボトムタブかどうか（rename_inputがSome時のみ有効）
    pub renaming_bottom_tab: bool,
}

impl OverlayState {
    /// 全オーバーレイを非表示にした初期状態を生成する
    pub fn new() -> Self {
        Self {
            command_palette: None,
            file_preview: None,
            file_prompt: None,
            file_info: None,
            status_message: None,
            rename_input: None,
            help_visible: false,
            renaming_bottom_tab: false,
        }
    }

    /// ステータスバーに一時メッセージ（Info）を設定する。
    /// 約3秒後に自動的に消える。
    ///
    /// @param text - 表示するメッセージ
    pub fn set_status_message(&mut self, text: String) {
        self.set_status_message_with_level(text, MessageLevel::Info);
    }

    /// ステータスバーに重要度付きの一時メッセージを設定する。
    /// 約3秒後に自動的に消える。
    ///
    /// @param text - 表示するメッセージ
    /// @param level - メッセージの重要度
    pub fn set_status_message_with_level(&mut self, text: String, level: MessageLevel) {
        self.status_message = Some(StatusMessage {
            text,
            level,
            remaining_frames: 60, // 約3秒（50ms×60）
        });
    }

    /// 一時メッセージの残りフレーム数をカウントダウンし、0になったら消す
    pub fn tick_status_message(&mut self) {
        if let Some(ref mut msg) = self.status_message {
            msg.remaining_frames = msg.remaining_frames.saturating_sub(1);
            if msg.remaining_frames == 0 {
                self.status_message = None;
            }
        }
    }
}

/// ハイライト済みの行。各スパンはテキストとスタイル情報を持つ。
pub struct HighlightedLine {
    /// スタイル付きテキスト断片のリスト
    pub spans: Vec<(String, Style)>,
}

impl HighlightedLine {
    /// プレーンテキスト（ハイライトなし）の行を生成する
    fn plain(text: &str) -> Self {
        Self {
            spans: vec![(text.to_string(), Style::default().fg(theme::get().text_body))],
        }
    }
}

/// syntectの色をratuiのRGBカラーに変換するヘルパー
fn syntect_color_to_ratatui(color: syntect::highlighting::Color) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}

/// ファイルプレビューの状態を管理する構造体
pub struct FilePreview {
    /// プレビュー対象のファイルパス
    pub path: PathBuf,
    /// ハイライト済みの行リスト
    pub lines: Vec<HighlightedLine>,
    /// スクロール位置（先頭行のインデックス）
    pub scroll: usize,
    /// テキストファイルかどうか
    pub is_text: bool,
}

impl FilePreview {
    /// ファイルを読み込んでプレビュー状態を生成する。
    /// バイナリファイルの場合はis_text=falseで返す。
    /// テキストファイルはsyntectでシンタックスハイライトを適用する。
    ///
    /// @param path - プレビュー対象のファイルパス
    /// @returns FilePreviewインスタンス
    pub fn load(path: PathBuf) -> Self {
        let (lines, is_text) = match fs::read(&path) {
            Ok(bytes) => {
                // バイナリ判定: 先頭8KBにNULLバイトが含まれていればバイナリ
                let check_len = bytes.len().min(8192);
                if bytes[..check_len].contains(&0) {
                    let line = HighlightedLine {
                        spans: vec![("(binary file)".to_string(), Style::default().fg(theme::get().text_muted))],
                    };
                    (vec![line], false)
                } else {
                    let content = String::from_utf8_lossy(&bytes);
                    let raw_lines: Vec<&str> = content
                        .lines()
                        .take(PREVIEW_MAX_LINES)
                        .collect();
                    if raw_lines.is_empty() {
                        let line = HighlightedLine {
                            spans: vec![("(empty file)".to_string(), Style::default().fg(theme::get().text_body))],
                        };
                        (vec![line], true)
                    } else {
                        let highlighted = Self::highlight_lines(&path, &raw_lines);
                        (highlighted, true)
                    }
                }
            }
            Err(e) => {
                let line = HighlightedLine {
                    spans: vec![(format!("Read error: {e}"), Style::default().fg(theme::get().accent_error))],
                };
                (vec![line], false)
            }
        };

        FilePreview {
            path,
            lines,
            scroll: 0,
            is_text,
        }
    }

    /// syntectを使ってテキスト行にシンタックスハイライトを適用する。
    /// ハイライトに失敗した場合はプレーンテキストにフォールバックする。
    ///
    /// @param path - ファイルパス（拡張子からシンタックスを判定）
    /// @param raw_lines - ハイライト対象の行スライス
    /// @returns ハイライト済みの行リスト
    fn highlight_lines(path: &PathBuf, raw_lines: &[&str]) -> Vec<HighlightedLine> {
        use syntect::parsing::SyntaxSet;
        use syntect::highlighting::ThemeSet;
        use syntect::easy::HighlightLines;

        let ss = SyntaxSet::load_defaults_nonewlines();
        let ts = ThemeSet::load_defaults();

        // ダークテーマを選択
        let theme = match ts.themes.get("base16-ocean.dark") {
            Some(t) => t,
            None => {
                // テーマが見つからない場合はプレーンテキストにフォールバック
                return raw_lines.iter().map(|l| HighlightedLine::plain(l)).collect();
            }
        };

        // ファイル拡張子からシンタックスを判定
        let syntax = ss
            .find_syntax_for_file(path)
            .ok()
            .flatten()
            .unwrap_or_else(|| ss.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, theme);

        raw_lines
            .iter()
            .map(|line| {
                match highlighter.highlight_line(line, &ss) {
                    Ok(ranges) => {
                        let spans: Vec<(String, Style)> = ranges
                            .iter()
                            .map(|(style, text)| {
                                let fg = syntect_color_to_ratatui(style.foreground);
                                let mut ratatui_style = Style::default().fg(fg);
                                // 背景色はオーバーレイのSURFACE_OVERLAYを使うため設定しない
                                // ただしボールド・イタリックは反映する
                                if style.font_style.contains(syntect::highlighting::FontStyle::BOLD) {
                                    ratatui_style = ratatui_style.add_modifier(ratatui::style::Modifier::BOLD);
                                }
                                if style.font_style.contains(syntect::highlighting::FontStyle::ITALIC) {
                                    ratatui_style = ratatui_style.add_modifier(ratatui::style::Modifier::ITALIC);
                                }
                                if style.font_style.contains(syntect::highlighting::FontStyle::UNDERLINE) {
                                    ratatui_style = ratatui_style.add_modifier(ratatui::style::Modifier::UNDERLINED);
                                }
                                (text.to_string(), ratatui_style)
                            })
                            .collect();
                        if spans.is_empty() {
                            HighlightedLine::plain(line)
                        } else {
                            HighlightedLine { spans }
                        }
                    }
                    // ハイライト失敗時はプレーンテキストにフォールバック
                    Err(_) => HighlightedLine::plain(line),
                }
            })
            .collect()
    }
}

/// ファイル操作プロンプトの状態を管理する構造体
pub struct FilePrompt {
    /// プロンプトの種別（作成/リネーム/削除）
    pub kind: FilePromptKind,
    /// ユーザーの入力テキスト
    pub input: String,
}

/// ファイル詳細情報の表示状態
pub struct FileInfo {
    /// 対象ファイルのパス
    pub path: PathBuf,
    /// 表示用の情報行リスト
    pub lines: Vec<(String, String)>,
}

impl FileInfo {
    /// ファイルのメタデータを読み込んでFileInfoを生成する。
    ///
    /// @param path - 対象ファイルのパス
    /// @returns FileInfoインスタンス
    pub fn load(path: PathBuf) -> Self {
        let mut lines = Vec::new();

        lines.push(("Name".to_string(), path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "-".to_string())));
        lines.push(("Path".to_string(), path.to_string_lossy().to_string()));

        match fs::metadata(&path) {
            Ok(meta) => {
                // ファイルタイプ
                let file_type = if meta.is_dir() {
                    "Directory"
                } else if meta.is_symlink() {
                    "Symlink"
                } else {
                    "File"
                };
                lines.push(("Type".to_string(), file_type.to_string()));

                // サイズ
                let size = meta.len();
                let size_str = if size < 1024 {
                    format!("{size} B")
                } else if size < 1024 * 1024 {
                    format!("{:.1} KB", size as f64 / 1024.0)
                } else if size < 1024 * 1024 * 1024 {
                    format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
                } else {
                    format!("{:.1} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
                };
                lines.push(("Size".to_string(), size_str));

                // 更新日時
                if let Ok(modified) = meta.modified() {
                    let datetime: chrono::DateTime<chrono::Local> = modified.into();
                    lines.push(("Modified".to_string(),
                        datetime.format("%Y-%m-%d %H:%M").to_string()));
                }

                // パーミッション（Unix）
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = meta.permissions().mode();
                    lines.push(("Permissions".to_string(), format!("{:o}", mode & 0o777)));
                }

                // 行数（テキストファイルのみ、小さいファイル限定）
                if meta.is_file() && size < 1_000_000
                    && let Ok(content) = fs::read_to_string(&path) {
                        let line_count = content.lines().count();
                        lines.push(("Lines".to_string(), format!("{line_count}")));
                    }
            }
            Err(e) => {
                lines.push(("Error".to_string(), format!("{e}")));
            }
        }

        FileInfo { path, lines }
    }
}

/// ステータスメッセージの重要度。表示色の決定に使用する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageLevel {
    /// 通常の通知（コピー成功等）
    Info,
    /// 注意が必要な通知（ツール未検出等）
    Warn,
    /// エラー通知（ファイル操作失敗等）
    Error,
}

/// ステータスバーの一時メッセージ（コピー成功通知など）
pub struct StatusMessage {
    /// メッセージテキスト
    pub text: String,
    /// メッセージの重要度
    pub level: MessageLevel,
    /// 残り表示フレーム数
    pub remaining_frames: u32,
}

/// コマンドパレットの1エントリ。
/// 表示名・説明と、選択時に発行するActionを保持する。
pub struct CommandEntry {
    /// コマンド名（フィルタリング対象）
    pub name: String,
    /// コマンドの短い説明
    pub description: String,
    /// 選択時に発行するAction
    pub action: Action,
}

/// コマンドパレットの状態管理。
/// 入力テキストでフィルタリングし、候補リストから選択する。
pub struct CommandPalette {
    /// ユーザーの入力テキスト
    pub input: String,
    /// 登録済みコマンド一覧
    pub commands: Vec<CommandEntry>,
    /// フィルタリング後の候補インデックス
    pub filtered: Vec<usize>,
    /// 選択中の候補位置（filtered内のインデックス）
    pub selected: usize,
}

impl CommandPalette {
    /// 組み込みコマンドでコマンドパレットを初期化する
    pub fn new() -> Self {
        let commands = vec![
            CommandEntry {
                name: "quit".to_string(),
                description: "Quit kuroko".to_string(),
                action: Action::Quit,
            },
            CommandEntry {
                name: "terminal".to_string(),
                description: "Toggle terminal panel".to_string(),
                action: Action::ToggleTerminal,
            },
            CommandEntry {
                name: "files".to_string(),
                description: "Toggle file tree panel".to_string(),
                action: Action::ToggleFileTree,
            },
            CommandEntry {
                name: "git".to_string(),
                description: "Toggle git panel".to_string(),
                action: Action::ToggleGitPanel,
            },
            CommandEntry {
                name: "new-tab".to_string(),
                description: "Open new agent tab".to_string(),
                action: Action::NewTab,
            },
            CommandEntry {
                name: "close-tab".to_string(),
                description: "Close active tab".to_string(),
                action: Action::CloseTab,
            },
            CommandEntry {
                name: "next-tab".to_string(),
                description: "Switch to next tab".to_string(),
                action: Action::NextTab,
            },
            CommandEntry {
                name: "prev-tab".to_string(),
                description: "Switch to previous tab".to_string(),
                action: Action::PrevTab,
            },
            CommandEntry {
                name: "new-terminal".to_string(),
                description: "Open new terminal tab".to_string(),
                action: Action::NewTerminalTab,
            },
            CommandEntry {
                name: "help".to_string(),
                description: "Show keybindings help".to_string(),
                action: Action::ShowHelp,
            },
        ];

        let filtered: Vec<usize> = (0..commands.len()).collect();
        Self {
            input: String::new(),
            commands,
            filtered,
            selected: 0,
        }
    }

    /// 入力テキストで候補をフィルタリングし、選択位置をリセットする
    pub fn update_filter(&mut self) {
        let query = self.input.to_lowercase();
        self.filtered = self.commands.iter().enumerate()
            .filter(|(_, cmd)| {
                if query.is_empty() {
                    return true;
                }
                cmd.name.to_lowercase().contains(&query)
                    || cmd.description.to_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect();
        // 選択位置を範囲内に収める
        if self.filtered.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(self.filtered.len() - 1);
        }
    }

    /// 選択中のコマンドのActionを返す
    pub fn selected_action(&self) -> Option<Action> {
        self.filtered.get(self.selected)
            .map(|&idx| self.commands[idx].action.clone())
    }

    /// 選択を1つ下へ
    pub fn move_down(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1).min(self.filtered.len() - 1);
        }
    }

    /// 選択を1つ上へ
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
}
