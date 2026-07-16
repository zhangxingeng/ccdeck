//! Extract searchable text from a session JSONL, mirroring the entry/block
//! classification semantics of `src/lib/parser.ts` (`parseJsonl` +
//! `extractContentBlocks`).
//!
//! **Messages only (v3, issue #35):** we index only what was *said* — `user`
//! and `assistant` **text** blocks. Thinking, tool_use, and tool_result blocks
//! are deliberately not extracted: they're the bulk of the corpus (tool_result
//! bodies especially) and users search for conversation, not tool noise. So a
//! block's `source` is now always `"user"` or `"assistant"`.
//!
//! `block_no` counts only the text blocks this extractor emits, so within one
//! message it's the index among *rendered* text blocks — which now lines up
//! with the frontend (post the Phase-A render-trim, see `ARCHITECTURE.md`, it
//! also only produces `'text'` blocks). Nothing relies on that alignment for
//! positioning today — jump-to-hit navigates by `uuid` alone, and
//! `line_no`/`block_no` are only ever used as dedup-key strings.
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
    /// `"user"` or `"assistant"` — the speaker (drives the You/Claude hit badge).
    /// No longer any of thinking/tool_use/tool_result: those aren't indexed.
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

/// Port of `extractContentBlocks`, narrowed to **text blocks only** (v3, issue
/// #35): `text_source` (`"user"` or `"assistant"`) is the block's source.
/// Thinking, tool_use, and tool_result blocks are skipped entirely — we index
/// conversation, not tool noise. `block_no` is the index among the text blocks
/// this extractor emits (see the module doc — it's a dedup key, not a position).
fn extract_content_blocks(arr: &[Value], text_source: &str) -> Vec<(i64, String, String)> {
    let mut out = Vec::new();
    let mut block_no: i64 = 0;
    for b in arr {
        if !b.is_object() {
            continue;
        }
        if b.get("type").and_then(Value::as_str) == Some("text") {
            let t = b.get("text").and_then(Value::as_str).unwrap_or("");
            out.push((block_no, text_source.to_string(), t.to_string()));
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
    fn extracts_only_message_text_blocks() {
        let blocks = extract_blocks(FIXTURE);

        // Messages only (v3): user string (l2) + assistant *text* (l3) = 2.
        // The l3 thinking + tool_use blocks and the l4 tool_result are all
        // skipped now, so they contribute nothing.
        assert_eq!(blocks.len(), 2, "got: {blocks:#?}");

        // user message
        assert_eq!(blocks[0].source, "user");
        assert_eq!(blocks[0].line_no, 2);
        assert_eq!(blocks[0].block_no, 0);
        assert_eq!(blocks[0].uuid, "u1");
        assert_eq!(blocks[0].text, "find the bug in parser");
        assert_eq!(blocks[0].ts, Some(1_782_986_400_000)); // 2026-07-02T10:00:00Z

        // assistant text — block_no 0 (the skipped thinking block no longer
        // advances the counter, so text is now the first emitted block).
        assert_eq!(blocks[1].source, "assistant");
        assert_eq!(blocks[1].line_no, 3);
        assert_eq!(blocks[1].block_no, 0);
        assert_eq!(blocks[1].uuid, "a1");
        assert_eq!(blocks[1].text, "I'll read the file");

        // Nothing thinking/tool-shaped leaks into the index.
        assert!(
            blocks
                .iter()
                .all(|b| b.source == "user" || b.source == "assistant"),
            "only user/assistant sources are indexed"
        );
    }

    #[test]
    fn empty_and_garbage_lines_are_skipped() {
        assert!(extract_blocks("").is_empty());
        assert!(extract_blocks("not json\n{bad").is_empty());
        assert!(extract_blocks("\n\n\n").is_empty());
    }
}
