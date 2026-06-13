//! kuroko-core: ペイン・レイアウト・イベント・アクションのコア抽象を定義するクレート。
//! アプリケーション全体の共通型とトレイトを提供する。

mod action;
pub mod error;
mod event;
pub mod layout;
mod pane;
pub mod theme;
mod types;

pub use action::{Action, FilePromptKind};
pub use error::KurokoError;
pub use event::AppEvent;
pub use layout::LayoutNode;
pub use pane::Pane;
pub use types::{Direction, PaneId, PaneType, SideContent};
