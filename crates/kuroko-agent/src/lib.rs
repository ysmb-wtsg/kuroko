//! kuroko-agent: AIエージェント統合ペイン。
//! Claude Code をラップし、PTY経由で操作する。

mod agent_pane;
mod claude_command;
pub mod claude_sessions;
mod session_list_pane;
mod session_pins;
mod status;

pub use agent_pane::AgentPane;
pub use claude_command::{AGENT_NAME, SessionStart};
pub use session_list_pane::{OpenSession, SessionListPane};
pub use status::{ActivityTracker, AgentStatus};
