//! Claude Code のセッション履歴の読み取り。
//! Claude Code は `~/.claude/projects/<エンコード済みcwd>/<session-id>.jsonl` に
//! 会話を1行1JSONで追記していく。ここではセッション一覧と表示用タイトルを抽出する。

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 検索ヒットとして保持する文脈の、マッチ前後の文字数。
/// 表示側が画面幅に合わせてさらに切り詰めるので、幅の広いサイドバーにも足りる長さにしておく
const SNIPPET_WIDTH: usize = 60;

/// 一覧表示用のセッション情報
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    /// セッションID（jsonlファイル名の拡張子を除いた部分。`claude --resume` に渡す）
    pub id: String,
    /// 表示用タイトル
    pub title: String,
    /// 最終更新時刻（jsonlファイルのmtime）
    pub modified: SystemTime,
}

/// タイトル解析結果のキャッシュ。キーはjsonlのパス、値は（mtime, サイズ, タイトル）。
/// 大きなjsonlを毎回読み直さないよう、mtimeとサイズが変わったファイルだけ再解析する。
pub type TitleCache = HashMap<PathBuf, (SystemTime, u64, Option<String>)>;

/// Claude Code の設定ディレクトリ（`$CLAUDE_CONFIG_DIR`、未設定なら `~/.claude`）
fn claude_config_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR")
        && !dir.is_empty()
    {
        return Some(PathBuf::from(dir));
    }
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".claude"))
}

/// 作業ディレクトリをClaude Codeのプロジェクトディレクトリ名に変換する。
/// 英数字以外を `-` に置き換える（例: `/Users/a/.config` → `-Users-a--config`）。
pub fn encode_project_path(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// 指定作業ディレクトリのセッション保存先ディレクトリを返す
pub fn project_sessions_dir(cwd: &Path) -> Option<PathBuf> {
    claude_config_dir().map(|dir| dir.join("projects").join(encode_project_path(cwd)))
}

/// ディレクトリ内のセッションを新しい順に列挙する。
/// タイトルを抽出できないセッション（`/clear` 直後で会話が無い等）は除外する。
///
/// @param dir - セッションjsonlが置かれたディレクトリ
/// @param cache - タイトル解析結果のキャッシュ（呼び出し間で使い回す）
/// @returns 更新日時の降順に並んだセッション一覧
pub fn list_sessions(dir: &Path, cache: &mut TitleCache) -> Vec<SessionInfo> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut sessions = Vec::new();
    let mut seen = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(id) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let len = meta.len();
        let title = match cache.get(&path) {
            Some((m, l, title)) if *m == modified && *l == len => title.clone(),
            _ => {
                let title = fs::File::open(&path)
                    .ok()
                    .and_then(|f| extract_title(BufReader::new(f)));
                cache.insert(path.clone(), (modified, len, title.clone()));
                title
            }
        };
        seen.push(path);
        if let Some(title) = title {
            sessions.push(SessionInfo {
                id,
                title,
                modified,
            });
        }
    }
    // 消えたファイルのキャッシュを捨てる
    cache.retain(|path, _| seen.contains(path));
    sessions.sort_by_key(|s| std::cmp::Reverse(s.modified));
    sessions
}

/// セッションjsonlから表示用タイトルを抽出する。
/// 優先順位: ユーザーが付けた名前（`custom-title`）→ 自動生成タイトル（`ai-title`）
/// → 直近のプロンプト（`last-prompt`）→ 最初のユーザー発言。いずれも後に出たものを優先する。
///
/// @param reader - jsonlの読み取り元
/// @returns タイトル。会話が無ければNone
pub fn extract_title(reader: impl BufRead) -> Option<String> {
    let mut custom = None;
    let mut ai = None;
    let mut last_prompt = None;
    let mut first_user = None;
    for line in reader.lines().map_while(Result::ok) {
        // 全行をJSONとして解析すると重いため、対象の種別を含む行だけ解析する
        let kind = if line.contains("\"custom-title\"") {
            "custom-title"
        } else if line.contains("\"ai-title\"") {
            "ai-title"
        } else if line.contains("\"last-prompt\"") {
            "last-prompt"
        } else if first_user.is_none() && line.contains("\"type\":\"user\"") {
            "user"
        } else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(|t| t.as_str()) != Some(kind) {
            continue;
        }
        let text = match kind {
            "custom-title" => value.get("customTitle").and_then(|v| v.as_str()),
            "ai-title" => value.get("aiTitle").and_then(|v| v.as_str()),
            "last-prompt" => value.get("lastPrompt").and_then(|v| v.as_str()),
            _ => user_message_text(&value),
        };
        let Some(text) = text.map(normalize_title).filter(|t| !t.is_empty()) else {
            continue;
        };
        match kind {
            "custom-title" => custom = Some(text),
            "ai-title" => ai = Some(text),
            "last-prompt" => last_prompt = Some(text),
            _ => first_user = Some(text),
        }
    }
    custom.or(ai).or(last_prompt).or(first_user)
}

/// ユーザー発言エントリから本文テキストを取り出す。
/// メタ情報（`isMeta`）やスラッシュコマンド等のタグ付き本文（`<` 始まり）は除外する。
fn user_message_text(value: &serde_json::Value) -> Option<&str> {
    if value.get("isMeta").and_then(|v| v.as_bool()) == Some(true) {
        return None;
    }
    let content = value.get("message")?.get("content")?;
    let text = match content {
        serde_json::Value::String(s) => s.as_str(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .find(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())?,
        _ => return None,
    };
    (!text.trim_start().starts_with('<')).then_some(text)
}

/// タイトルを1行に整える（改行・連続空白を1つの空白にまとめる）
fn normalize_title(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// アシスタント発言エントリから本文テキストを取り出す（`text` パートのみ。thinking・tool_use は除外）
fn assistant_message_text(value: &serde_json::Value) -> Option<String> {
    let content = value.get("message")?.get("content")?;
    match content {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(parts) => {
            let texts: Vec<&str> = parts
                .iter()
                .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect();
            (!texts.is_empty()).then(|| texts.join("\n"))
        }
        _ => None,
    }
}

/// jsonl の1行から、会話として見える本文（ユーザー発言またはアシスタント発言）を取り出す。
/// ツール入出力（`tool_result` 等）、メタ情報、スラッシュコマンド等のタグ付き本文は含めない。
///
/// @param line - jsonl の1行
/// @returns 本文。会話でない行や本文が無い行はNone
pub fn message_text(line: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    match value.get("type").and_then(|t| t.as_str())? {
        "user" => user_message_text(&value).map(str::to_string),
        "assistant" => assistant_message_text(&value),
        _ => None,
    }
}

/// 本文からクエリ周辺の抜粋を作る。大文字小文字は区別しない。
///
/// @param text - 本文
/// @param query - 検索語（空文字は一致しない）
/// @param width - マッチ前後に残す文字数
/// @returns 一致があれば、前後を `…` で切り詰めた1行の抜粋
pub fn snippet(text: &str, query: &str, width: usize) -> Option<String> {
    if query.is_empty() {
        return None;
    }
    let flat: Vec<char> = normalize_title(text).chars().collect();
    let needle: Vec<char> = query.to_lowercase().chars().collect();
    let lower: Vec<char> = flat
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let pos = lower.windows(needle.len()).position(|w| w == needle)?;
    let start = pos.saturating_sub(width);
    let end = (pos + needle.len() + width).min(flat.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(&flat[start..end]);
    if end < flat.len() {
        out.push('…');
    }
    Some(out)
}

/// 指定セッションの会話本文をクエリで検索し、ヒットしたセッションごとに最初の抜粋を返す。
/// 全行を JSON 解析すると重いため、生の行に（小文字化した）クエリが含まれる行だけを解析する。
///
/// @param dir - セッションjsonlが置かれたディレクトリ
/// @param ids - 検索対象のセッションID
/// @param query - 検索語
/// @returns セッションID → 抜粋
pub fn search_content<'a>(
    dir: &Path,
    ids: impl IntoIterator<Item = &'a str>,
    query: &str,
) -> HashMap<String, String> {
    let mut hits = HashMap::new();
    let query = query.trim();
    if query.is_empty() {
        return hits;
    }
    let needle = query.to_lowercase();
    for id in ids {
        let Ok(file) = fs::File::open(dir.join(format!("{id}.jsonl"))) else {
            continue;
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            // Claude Code はJSON文字列で非ASCIIをエスケープしないため、生の行での事前判定で足りる
            if !line.to_lowercase().contains(&needle) {
                continue;
            }
            if let Some(text) = message_text(&line)
                && let Some(found) = snippet(&text, query, SNIPPET_WIDTH)
            {
                hits.insert(id.to_string(), found);
                break;
            }
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title_of(lines: &[&str]) -> Option<String> {
        extract_title(lines.join("\n").as_bytes())
    }

    #[test]
    fn encode_replaces_non_alphanumerics() {
        assert_eq!(
            encode_project_path(Path::new("/Users/a/.claude")),
            "-Users-a--claude"
        );
        assert_eq!(
            encode_project_path(Path::new("/Users/a/projects/kuroko")),
            "-Users-a-projects-kuroko"
        );
    }

    #[test]
    fn title_prefers_custom_then_ai_title() {
        let lines = [
            r#"{"type":"user","message":{"role":"user","content":"first prompt"}}"#,
            r#"{"type":"last-prompt","lastPrompt":"latest prompt"}"#,
            r#"{"type":"ai-title","aiTitle":"Old title"}"#,
            r#"{"type":"ai-title","aiTitle":"New title"}"#,
        ];
        assert_eq!(title_of(&lines).as_deref(), Some("New title"));

        let mut with_custom = lines.to_vec();
        with_custom.push(r#"{"type":"custom-title","customTitle":"My name"}"#);
        assert_eq!(title_of(&with_custom).as_deref(), Some("My name"));
    }

    #[test]
    fn title_falls_back_to_last_prompt_then_first_user_message() {
        let lines = [
            r#"{"type":"user","message":{"role":"user","content":"first prompt"}}"#,
            r#"{"type":"last-prompt","lastPrompt":"latest\nprompt"}"#,
        ];
        assert_eq!(title_of(&lines).as_deref(), Some("latest prompt"));

        let lines = [
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"meta"}}"#,
            r#"{"type":"user","message":{"role":"user","content":"<command-name>/clear</command-name>"}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"hello there"}]}}"#,
        ];
        assert_eq!(title_of(&lines).as_deref(), Some("hello there"));
    }

    #[test]
    fn title_is_none_without_conversation() {
        let lines = [
            r#"{"type":"mode","sessionId":"x"}"#,
            r#"{"type":"user","message":{"role":"user","content":"<command-name>/clear</command-name>"}}"#,
        ];
        assert_eq!(title_of(&lines), None);
    }

    #[test]
    fn message_text_extracts_conversation_only() {
        let user = r#"{"type":"user","message":{"role":"user","content":"hello"}}"#;
        assert_eq!(message_text(user).as_deref(), Some("hello"));

        let assistant = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"answer"},{"type":"tool_use","name":"Bash"}]}}"#;
        assert_eq!(message_text(assistant).as_deref(), Some("answer"));

        // ツール結果（userとして記録される）・メタ・タグ付き本文・非会話行は除外
        let tool_result = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"secret"}]}}"#;
        assert_eq!(message_text(tool_result), None);
        let meta = r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"meta"}}"#;
        assert_eq!(message_text(meta), None);
        let tagged = r#"{"type":"user","message":{"role":"user","content":"<command-name>/x</command-name>"}}"#;
        assert_eq!(message_text(tagged), None);
        assert_eq!(message_text(r#"{"type":"ai-title","aiTitle":"x"}"#), None);
        assert_eq!(message_text("not json"), None);
    }

    #[test]
    fn snippet_trims_around_match_and_is_case_insensitive() {
        let text = "aaaaaaaaaa Needle bbbbbbbbbb";
        assert_eq!(
            snippet(text, "needle", 3).as_deref(),
            Some("…aa Needle bb…")
        );
        assert_eq!(
            snippet("short Needle", "NEEDLE", 20).as_deref(),
            Some("short Needle")
        );
        assert_eq!(snippet(text, "zzz", 3), None);
        assert_eq!(snippet(text, "", 3), None);
    }

    #[test]
    fn snippet_handles_multibyte_text() {
        let text = "あいうえおかきくけこ認証トークンさしすせそたちつてと";
        assert_eq!(snippet(text, "認証", 2).as_deref(), Some("…けこ認証トー…"));
    }

    #[test]
    fn search_content_finds_conversation_text_not_tool_output() {
        let dir = std::env::temp_dir().join(format!("krk-search-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("a.jsonl"),
            concat!(
                r#"{"type":"user","message":{"role":"user","content":"fix the Login bug"}}"#,
                "\n",
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"done"}]}}"#,
            ),
        )
        .unwrap();
        fs::write(
            dir.join("b.jsonl"),
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"login in output"}]}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("c.jsonl"),
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"認証を直した"}]}}"#,
        )
        .unwrap();

        let hits = search_content(&dir, ["a", "b", "c", "missing"], "login");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits["a"], "fix the Login bug");

        let hits = search_content(&dir, ["a", "c"], "認証");
        assert_eq!(hits.keys().collect::<Vec<_>>(), vec!["c"]);

        assert!(search_content(&dir, ["a"], "  ").is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn list_sessions_sorts_by_mtime_and_skips_empty() {
        let dir = std::env::temp_dir().join(format!("krk-sessions-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("old.jsonl"),
            r#"{"type":"ai-title","aiTitle":"Old"}"#,
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(
            dir.join("new.jsonl"),
            r#"{"type":"ai-title","aiTitle":"New"}"#,
        )
        .unwrap();
        fs::write(dir.join("empty.jsonl"), r#"{"type":"mode"}"#).unwrap();
        fs::write(dir.join("note.txt"), "ignored").unwrap();

        let mut cache = TitleCache::new();
        let sessions = list_sessions(&dir, &mut cache);
        let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["new", "old"]);
        assert_eq!(cache.len(), 3);

        fs::remove_dir_all(&dir).unwrap();
    }
}
