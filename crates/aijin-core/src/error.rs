//! aijinアプリケーション全体で使用するドメインエラー型。
//! ライブラリクレートでの型安全なエラー伝播を提供する。

use thiserror::Error;

/// aijinアプリケーションのドメインエラー。
/// 各サブシステム（PTY、Lua、IO）のエラーを統一的に表現する。
#[derive(Debug, Error)]
pub enum AijinError {
    /// PTY操作（生成、書き込み、リサイズ）のエラー
    #[error("PTY error: {0}")]
    Pty(String),

    /// Luaランタイム（初期化、API登録、ファイル実行）のエラー
    #[error("Lua error: {0}")]
    Lua(String),

    /// IO操作のエラー
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
