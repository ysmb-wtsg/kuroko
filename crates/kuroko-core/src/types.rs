//! アプリケーション全体で使用する基本型の定義。
//! PaneId、Direction、PaneTypeなどの列挙型を提供する。

/// ペインを一意に識別するID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(pub u64);

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
