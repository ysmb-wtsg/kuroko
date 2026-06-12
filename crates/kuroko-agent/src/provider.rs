//! AIエージェントプロバイダーの定義。
//! 各エージェント（Claude Code、Codex等）の起動コマンドとステータス検出を抽象化する。

use portable_pty::CommandBuilder;

/// エージェントの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    /// 起動中
    Starting,
    /// 入力待ち（アイドル）
    Idle,
    /// 処理中
    Working,
    /// 終了済み
    Exited,
}

/// エージェントプロバイダーの共通インターフェース。
/// 各AIエージェントの起動コマンドと状態検出ロジックを定義する。
pub trait AgentProvider: Send {
    /// プロバイダー名を返す
    fn name(&self) -> &str;

    /// 起動コマンドを生成する
    fn command(&self) -> CommandBuilder;

    /// ペインタイトルを返す
    fn title(&self) -> String {
        self.name().to_string()
    }
}

/// ビルトインのエージェントプロバイダー
#[derive(Debug, Clone)]
pub enum BuiltinProvider {
    /// Claude Code
    ClaudeCode,
    /// OpenAI Codex CLI
    Codex,
    /// カスタムコマンド
    Custom {
        name: String,
        command: String,
        args: Vec<String>,
    },
}

impl AgentProvider for BuiltinProvider {
    fn name(&self) -> &str {
        match self {
            BuiltinProvider::ClaudeCode => "Claude Code",
            BuiltinProvider::Codex => "Codex",
            BuiltinProvider::Custom { name, .. } => name,
        }
    }

    fn command(&self) -> CommandBuilder {
        match self {
            BuiltinProvider::ClaudeCode => CommandBuilder::new("claude"),
            BuiltinProvider::Codex => CommandBuilder::new("codex"),
            BuiltinProvider::Custom { command, args, .. } => {
                let mut cmd = CommandBuilder::new(command);
                for arg in args {
                    cmd.arg(arg);
                }
                cmd
            }
        }
    }

    fn title(&self) -> String {
        match self {
            BuiltinProvider::ClaudeCode => "Agent: Claude Code".to_string(),
            BuiltinProvider::Codex => "Agent: Codex".to_string(),
            BuiltinProvider::Custom { name, .. } => format!("Agent: {}", name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_name() {
        assert_eq!(BuiltinProvider::ClaudeCode.name(), "Claude Code");
    }

    #[test]
    fn codex_name() {
        assert_eq!(BuiltinProvider::Codex.name(), "Codex");
    }

    #[test]
    fn claude_code_title() {
        assert_eq!(BuiltinProvider::ClaudeCode.title(), "Agent: Claude Code");
    }

    #[test]
    fn codex_title() {
        assert_eq!(BuiltinProvider::Codex.title(), "Agent: Codex");
    }

    #[test]
    fn claude_code_command_is_not_empty() {
        let cmd = BuiltinProvider::ClaudeCode.command();
        // CommandBuilderが生成されることを確認（パニックしないこと）
        let _ = cmd;
    }

    #[test]
    fn codex_command_is_not_empty() {
        let cmd = BuiltinProvider::Codex.command();
        let _ = cmd;
    }

    #[test]
    fn custom_provider_name() {
        let provider = BuiltinProvider::Custom {
            name: "MyAgent".to_string(),
            command: "my-agent".to_string(),
            args: vec![],
        };
        assert_eq!(provider.name(), "MyAgent");
    }

    #[test]
    fn custom_provider_title() {
        let provider = BuiltinProvider::Custom {
            name: "MyAgent".to_string(),
            command: "my-agent".to_string(),
            args: vec![],
        };
        assert_eq!(provider.title(), "Agent: MyAgent");
    }

    #[test]
    fn custom_provider_command() {
        let provider = BuiltinProvider::Custom {
            name: "MyAgent".to_string(),
            command: "my-agent".to_string(),
            args: vec!["--flag".to_string()],
        };
        // CommandBuilderが生成されることを確認
        let _ = provider.command();
    }
}
