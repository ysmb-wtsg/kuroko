//! アプリケーション全体で使用する基本型の定義。
//! PaneId、Mode、Direction、PaneTypeなどの列挙型を提供する。

use std::fmt;

/// ペインを一意に識別するID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(pub u64);

/// アプリケーションの入力モード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Vimライクなキーバインドでペイン操作・フォーカス移動を行うモード
    Normal,
    /// キー入力をフォーカス中のPTYペインに直接転送するモード
    Insert,
    /// ファイルツリーでの複数選択モード
    Select,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mode::Normal => write!(f, "NORMAL"),
            Mode::Insert => write!(f, "INSERT"),
            Mode::Select => write!(f, "SELECT"),
        }
    }
}

/// 分割・フォーカス移動の方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// ペインの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneType {
    /// シェルターミナル
    Terminal,
    /// AIエージェント
    Agent,
    /// ファイルツリー
    FileTree,
}

/// サイドパネル（右）に表示するサブパネルの種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideContent {
    /// ファイルツリー
    FileTree,
    /// 外部Gitツール（lazygit等）
    Git,
}
