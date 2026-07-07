//! Extract searchable text from a session JSONL, mirroring the entry/block
//! classification semantics of `src/lib/parser.ts` (`parseJsonl` +
//! `extractContentBlocks`). `block_no` does **not** line up with the editor's
//! block numbering: this extractor still counts across all four historical
//! block types (thinking/text/tool_use/tool_result), while the frontend
//! (post the Phase-A render-trim, see `ARCHITECTURE.md`) only ever produces
//! `'text'` blocks. Nothing in the app relies on `block_no` for cross-
//! referencing or positioning today — jump-to-hit navigates by `uuid` alone,
//! and `line_no`/`block_no` are only ever used as dedup-key strings — but a
//! future feature reaching for real block-level positioning should not
//! assume the two numbering schemes agree.
//!
//! The output is a flat list of [`ExtractedBlock`]s — one per searchable
//! content block — which the indexer stages into the tantivy full-text index
//! (see `index.rs`; the extracted text no longer goes into a SQLite table).

use serde_json::Value;

/// Entry `type`s that carry no conversational content — skipped entirely.
/// Mirrors `META_TYPES` in parser.ts (plus `system`, handled inline there).
const META_TYPES: &[&str] = &[
    "mode",
    "permission-mode",
    "ai-title",
    "file-history-snapshot",
    "last-prompt",
    "queue-operation",
    "attachment",
    "bridge-session",
    "skill-listing",
    "deferred-tools-delta",
    "system",
];

/// Prefixes identifying internal command-echo messages — dropped, like parser.ts.
const INTERNAL_ECHO_PREFIXES: &[&str] = &[
    "<command-name>",
    "<local-command-stdout>",
    "<command-message>",
    "<command-args>",
    "<local-command-caveat>",
    "<system-reminder>",
    "<teammate-message",
    "<task-notification>",
];

/// One extracted, searchable content block — a row-to-be in the `blocks` table.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedBlock {
    /// 0-based physical line index in the JSONL (for debugging / stable order).
    pub line_no: i64,
    /// Index of this block within its message's rendered block list
    /// (matches the frontend's `entry.blocks` index — the key to jump-to-hit).
    pub block_no: i64,
    /// The source message's uuid (stable jump-to-hit anchor across turn regrouping).
    pub uuid: String,
    /// Message timestamp as epoch milliseconds, if parseable (for date filtering).
    pub ts: Option<i64>,
    /// 'user' | 'assistant' | 'thinking' | 'tool_use' | 'tool_result'.
    pub source: String,
    /// The extracted plain text to search.
    pub text: String,
}

/// Parse ISO-8601 / RFC-3339 (`2026-07-02T19:20:30.123Z`) to epoch milliseconds.
fn parse_ts(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

fn is_internal_echo_str(s: &str) -> bool {
    INTERNAL_ECHO_PREFIXES.iter().any(|p| s.starts_with(p))
}

/// Array content can also begin with an echo prefix in its first text block.
fn is_internal_echo_arr(arr: &[Value]) -> bool {
    let Some(first) = arr.first() else { return false };
    if first.get("type").and_then(Value::as_str) != Some("text") {
        return false;
    }
    match first.get("text").and_then(Value::as_str) {
        Some(t) => is_internal_echo_str(t),
        None => false,
    }
}

/// Recursively collect readable scalars from a tool-input JSON value, so a file
/// path or command buried inside a tool call is searchable. Object scalars are
/// rendered as `key: value` so field names are searchable too.
fn flatten_json(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Number(n) => out.push(n.to_string()),
        Value::Bool(b) => out.push(b.to_string()),
        Value::Array(a) => {
            for item in a {
                flatten_json(item, out);
            }
        }
        Value::Object(o) => {
            for (k, val) in o {
                match val {
                    Value::String(s) => out.push(format!("{k}: {s}")),
                    Value::Number(n) => out.push(format!("{k}: {n}")),
                    Value::Bool(b) => out.push(format!("{k}: {b}")),
                    _ => {
                        out.push(k.clone());
                        flatten_json(val, out);
                    }
                }
            }
        }
        Value::Null => {}
    }
}

/// tool_result text: join the `text` parts of an array content, or use a plain
/// string content directly. Mirrors parser.ts (which only handled arrays) but
/// additionally indexes string-form results so they're searchable.
fn extract_tool_result_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => {
            let parts: Vec<&str> = arr
                .iter()
                .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect();
            parts.join("\n")
        }
        _ => String::new(),
    }
}

/// Port of `extractContentBlocks`. `text_source` is the entry type ('user' or
/// 'assistant') used for plain text blocks; thinking/tool_use/tool_result carry
/// their own source. `block_no` is the index in this extractor's *own* output
/// block list (all four block types advance it) — see the module doc for why
/// that no longer matches the frontend's block numbering.
fn extract_content_blocks(arr: &[Value], text_source: &str) -> Vec<(i64, String, String)> {
    let mut out = Vec::new();
    let mut block_no: i64 = 0;
    for b in arr {
        if !b.is_object() {
            continue;
        }
        let btype = b.get("type").and_then(Value::as_str).unwrap_or("");
        let produced: Option<(String, String)> = match btype {
            "thinking" => {
                let t = b.get("thinking").and_then(Value::as_str).unwrap_or("");
                Some(("thinking".to_string(), t.to_string()))
            }
            "text" => {
                let t = b.get("text").and_then(Value::as_str).unwrap_or("");
                Some((text_source.to_string(), t.to_string()))
            }
            "tool_use" => {
                let name = b.get("name").and_then(Value::as_str).unwrap_or("unknown");
                let mut parts = Vec::new();
                if let Some(input) = b.get("input") {
                    flatten_json(input, &mut parts);
                }
                let text = if parts.is_empty() {
                    name.to_string()
                } else {
                    format!("{name}\n{}", parts.join("\n"))
                };
                Some(("tool_use".to_string(), text))
            }
            "tool_result" => {
                let text = extract_tool_result_text(b.get("content"));
                Some(("tool_result".to_string(), text))
            }
            _ => None,
        };
        if let Some((source, text)) = produced {
            out.push((block_no, source, text));
            block_no += 1;
        }
    }
    out
}

/// Extract the (block_no, source, text) tuples for a single parsed entry's
/// content, dispatching on entry `type`. Returns an empty vec for entries with
/// no searchable content (echoes, task-notifications, tool-result-only users).
fn extract_entry(typ: &str, content: Option<&Value>) -> Vec<(i64, String, String)> {
    match typ {
        "user" => match content {
            Some(Value::String(s)) => {
                if s.starts_with("<task-notification>") || is_internal_echo_str(s) {
                    vec![]
                } else if s.contains("[Request interrupted by user]") {
                    vec![(0, "user".to_string(), "[Request interrupted by user]".to_string())]
                } else {
                    vec![(0, "user".to_string(), s.clone())]
                }
            }
            Some(Value::Array(arr)) => {
                if is_internal_echo_arr(arr) {
                    vec![]
                } else {
                    extract_content_blocks(arr, "user")
                }
            }
            _ => vec![],
        },
        "assistant" => match content {
            Some(Value::Array(arr)) => extract_content_blocks(arr, "assistant"),
            _ => vec![],
        },
        _ => vec![],
    }
}

/// Extract every searchable block from a session's raw JSONL text.
pub fn extract_blocks(jsonl: &str) -> Vec<ExtractedBlock> {
    let mut out = Vec::new();

    for (idx, line) in jsonl.split('\n').enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let raw: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let typ = raw.get("type").and_then(Value::as_str).unwrap_or("");
        if META_TYPES.contains(&typ) {
            continue;
        }

        let uuid = raw
            .get("uuid")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let ts = raw
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(parse_ts);
        let content = raw.get("message").and_then(|m| m.get("content"));

        let line_no = idx as i64;
        for (block_no, source, text) in extract_entry(typ, content) {
            // Skip blocks with no searchable text (block_no was already
            // assigned, preserving alignment with the frontend's indices).
            if text.trim().is_empty() {
                continue;
            }
            out.push(ExtractedBlock {
                line_no,
                block_no,
                uuid: uuid.clone(),
                ts,
                source,
                text,
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-built fixture exercising each source + the skip rules. The block
    /// indices/sources must match what parser.ts would produce.
    const FIXTURE: &str = concat!(
        // line 0: a meta entry — skipped entirely.
        r#"{"type":"ai-title","message":{"content":"My Session"}}"#,
        "\n",
        // line 1: internal echo — skipped.
        r#"{"type":"user","uuid":"u0","message":{"content":"<system-reminder>ignore me</system-reminder>"}}"#,
        "\n",
        // line 2: plain user string message.
        r#"{"type":"user","uuid":"u1","timestamp":"2026-07-02T10:00:00.000Z","message":{"content":"find the bug in parser"}}"#,
        "\n",
        // line 3: assistant with thinking + text + tool_use (3 blocks).
        r#"{"type":"assistant","uuid":"a1","timestamp":"2026-07-02T10:00:05.000Z","message":{"content":[{"type":"thinking","thinking":"let me look"},{"type":"text","text":"I'll read the file"},{"type":"tool_use","name":"Read","input":{"file_path":"/src/parser.ts"}}]}}"#,
        "\n",
        // line 4: user with a tool_result only (no user text).
        r#"{"type":"user","uuid":"u2","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"line 42: off by one"}]}]}}"#,
        "\n",
        // line 5: blank line — skipped.
        "",
    );

    #[test]
    fn extracts_expected_blocks() {
        let blocks = extract_blocks(FIXTURE);

        // Expected: user string (l2), thinking+text+tool_use (l3), tool_result (l4) = 5.
        assert_eq!(blocks.len(), 5, "got: {blocks:#?}");

        // user message
        assert_eq!(blocks[0].source, "user");
        assert_eq!(blocks[0].line_no, 2);
        assert_eq!(blocks[0].block_no, 0);
        assert_eq!(blocks[0].uuid, "u1");
        assert_eq!(blocks[0].text, "find the bug in parser");
        assert_eq!(blocks[0].ts, Some(1_782_986_400_000)); // 2026-07-02T10:00:00Z

        // assistant thinking / text / tool_use — block_no 0,1,2 in order
        assert_eq!(blocks[1].source, "thinking");
        assert_eq!(blocks[1].block_no, 0);
        assert_eq!(blocks[1].text, "let me look");

        assert_eq!(blocks[2].source, "assistant");
        assert_eq!(blocks[2].block_no, 1);
        assert_eq!(blocks[2].text, "I'll read the file");

        assert_eq!(blocks[3].source, "tool_use");
        assert_eq!(blocks[3].block_no, 2);
        assert!(blocks[3].text.contains("Read"));
        assert!(blocks[3].text.contains("/src/parser.ts"));

        // tool_result from the user entry
        assert_eq!(blocks[4].source, "tool_result");
        assert_eq!(blocks[4].line_no, 4);
        assert_eq!(blocks[4].block_no, 0);
        assert_eq!(blocks[4].text, "line 42: off by one");
    }

    #[test]
    fn empty_and_garbage_lines_are_skipped() {
        assert!(extract_blocks("").is_empty());
        assert!(extract_blocks("not json\n{bad").is_empty());
        assert!(extract_blocks("\n\n\n").is_empty());
    }
}
