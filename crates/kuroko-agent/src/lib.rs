//! kuroko-agent: AIエージェント統合ペイン。
//! Claude Code、Codexなどのエージェントをラップし、PTY経由で操作する。

mod agent_pane;
mod provider;
mod status;

pub use agent_pane::AgentPane;
pub use provider::{AgentProvider, BuiltinProvider};
pub use status::{ActivityTracker, AgentStatus};
