//! aijin-terminal: PTYベースのターミナルエミュレーションペイン。
//! portable-ptyでシェルを起動し、vt100でANSIパース、ratatuiで描画する。

pub mod pty_handle;
mod terminal_pane;
pub mod widget;

pub use terminal_pane::TerminalPane;
