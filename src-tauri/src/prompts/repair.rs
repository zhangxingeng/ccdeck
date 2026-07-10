//! In-memory jsonrepair-style recovery for hand-edited JSON (contract
//! § Store robustness): unquoted keys, trailing commas, comments, single
//! quotes, truncation. Used by the piece loader and the project roster; both
//! honor the same invariant — **repair never rewrites the user's file**, the
//! repaired form persists only through the user's next explicit save.
//!
//! Crate choice (researched 2026-07): `llm_json`, the Rust port of the
//! widely-used python `json_repair` — org-backed (oramasearch), 1.x with a
//! steady release history; preferred over `jsonrepair` (a single 0.1.0
//! release) and `json-repair` (unmaintained since 2024).

use serde_json::Value;

/// Best-effort repair of `content` into a JSON value. Deliberately
/// string→string (`repair_json`) followed by a normal strict parse, so
/// serde_json stays the ONLY producer of `Value` semantics — the hostile
/// round-trip tests in `store` guard number exactness through this path.
/// `None` means unrecoverable; the caller reports the original strict error.
pub(super) fn repair_to_value(content: &str) -> Option<Value> {
    let repaired = llm_json::repair_json(content, &Default::default()).ok()?;
    serde_json::from_str(&repaired).ok()
}
