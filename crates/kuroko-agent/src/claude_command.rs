//! Claude Code の起動コマンド生成。
//! kuroko は Claude Code 専用のため、エージェントの抽象化は持たない（ADR 0013）。

use portable_pty::CommandBuilder;

/// エージェントの表示名（通知本文などに使う）
pub const AGENT_NAME: &str = "Claude Code";

/// Claude Code セッションの開始方法
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStart {
    /// 指定IDで新規セッションを開始する（`claude --session-id <id>`）
    New(String),
    /// 既存セッションを再開する（`claude --resume <id>`）
    Resume(String),
}

impl SessionStart {
    /// 新しいセッションIDを採番して新規開始を表す値を返す
    pub fn new_session() -> Self {
        SessionStart::New(uuid::Uuid::new_v4().to_string())
    }

    /// 対象のセッションID
    pub fn id(&self) -> &str {
        match self {
            SessionStart::New(id) | SessionStart::Resume(id) => id,
        }
    }
}

/// セッション指定付きで Claude Code の起動コマンドを生成する。
///
/// @param session - セッションの開始方法
/// @returns 起動コマンド
pub fn claude_command(session: &SessionStart) -> CommandBuilder {
    let mut cmd = CommandBuilder::new("claude");
    match session {
        SessionStart::New(id) => cmd.args(["--session-id", id]),
        SessionStart::Resume(id) => cmd.args(["--resume", id]),
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(cmd: &CommandBuilder) -> Vec<String> {
        cmd.get_argv()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn session_args() {
        let new = claude_command(&SessionStart::New("abc".to_string()));
        assert_eq!(argv(&new), vec!["claude", "--session-id", "abc"]);
        let resume = claude_command(&SessionStart::Resume("abc".to_string()));
        assert_eq!(argv(&resume), vec!["claude", "--resume", "abc"]);
    }

    #[test]
    fn new_session_ids_are_unique_uuids() {
        let a = SessionStart::new_session();
        let b = SessionStart::new_session();
        assert_ne!(a.id(), b.id());
        assert!(uuid::Uuid::parse_str(a.id()).is_ok());
    }
}
