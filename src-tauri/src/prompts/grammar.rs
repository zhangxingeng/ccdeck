//! The variable grammar (contract § Variable grammar): single-brace,
//! python-f-string flavored. SEAM-CRITICAL: the TS side implements this exact
//! spec, and both sides assert the contract's shared test vectors verbatim —
//! last round's audit caught the two lanes diverging on exactly this class of
//! rule, so any change here is a contract change, never a local fix.
//!
//! Scan left-to-right:
//! 1. `{{` / `}}` are escapes for literal braces — they consume first.
//! 2. `{name}` or `{name:default}` is a variable when `name` matches
//!    `[A-Za-z0-9_-]+` (case-sensitive). The FIRST `:` splits name from
//!    default; the default runs to the closing `}` and may not contain
//!    braces.
//! 3. Any other braced run is verbatim prose — JSON examples inside prompt
//!    bodies never parse as variables.
//! 4. One name is one variable document-wide; 5. when the same name carries
//!    differing defaults, the first occurrence's default wins
//!    (first-appearance order rules everything in this contract).

use serde::{Deserialize, Serialize};

/// One derived variable: `name` plus its optional default. `{x:}` yields
/// `Some("")` — "fills as empty when unfilled" — distinct from `{x}`'s
/// `None`, so the compose layer can tell "empty default" from "no default".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placeholder {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// Is `byte` legal in a variable name? ASCII `[A-Za-z0-9_-]` only —
/// deliberately narrower than the retired `{{token}}` grammar's `[\w.-]`
/// (dots are out), and never unicode, so prose like `{tickét}` stays prose.
fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

/// Derive the `placeholders` schema array from a body: every variable the
/// grammar finds, deduped by name in first-appearance order, each carrying
/// the first occurrence's default (rule 5).
pub fn derive_placeholders(body: &str) -> Vec<Placeholder> {
    let bytes = body.as_bytes();
    let mut out: Vec<Placeholder> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            // Escapes consume before variable parsing (rule 1) — this is
            // what makes `{{task}}` a literal and `{{{task}}}` an escape
            // plus a real variable.
            b'{' if bytes.get(i + 1) == Some(&b'{') => i += 2,
            b'}' if bytes.get(i + 1) == Some(&b'}') => i += 2,
            b'{' => match parse_variable(bytes, i) {
                Some((placeholder, end)) => {
                    if !out.iter().any(|p| p.name == placeholder.name) {
                        out.push(placeholder);
                    }
                    i = end;
                }
                None => i += 1, // verbatim prose brace (rule 3)
            },
            _ => i += 1,
        }
    }
    out
}

/// Try to read `{name}` / `{name:default}` from the `{` at `start`. Returns
/// the placeholder and the index just past the closing `}`; `None` means the
/// run is prose. Byte indexing is UTF-8-safe here: every delimiter tested is
/// ASCII, so slice bounds always land on char boundaries.
fn parse_variable(bytes: &[u8], start: usize) -> Option<(Placeholder, usize)> {
    let name_start = start + 1;
    let mut i = name_start;
    while i < bytes.len() && is_name_byte(bytes[i]) {
        i += 1;
    }
    if i == name_start {
        return None; // empty name: `{:x}`, `{"a"...`, `{ ...`
    }
    // Only ASCII name bytes were consumed, so this cannot fail.
    let name = String::from_utf8(bytes[name_start..i].to_vec()).ok()?;
    match bytes.get(i) {
        Some(b'}') => Some((Placeholder { name, default: None }, i + 1)),
        Some(b':') => {
            let default_start = i + 1;
            let mut j = default_start;
            while j < bytes.len() {
                match bytes[j] {
                    b'}' => {
                        let default =
                            String::from_utf8(bytes[default_start..j].to_vec()).ok()?;
                        return Some((Placeholder { name, default: Some(default) }, j + 1));
                    }
                    b'{' => return None, // a default may not contain braces
                    _ => j += 1,
                }
            }
            None // unclosed — prose
        }
        _ => None, // name interrupted by space, quote, unicode, EOF…
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(name: &str, default: Option<&str>) -> Placeholder {
        Placeholder { name: name.to_string(), default: default.map(str::to_string) }
    }

    /// The contract's shared test vectors, verbatim and in table order —
    /// the TS lane asserts these same inputs. Rows whose result is "literal"
    /// assert that NO variable is derived (literal emission is copy-time
    /// behavior, which lives on the frontend).
    #[test]
    fn shared_contract_vectors() {
        assert_eq!(derive_placeholders("{task}"), vec![var("task", None)]);
        assert_eq!(
            derive_placeholders("{task:write tests}"),
            vec![var("task", Some("write tests"))]
        );
        assert_eq!(derive_placeholders("{task:}"), vec![var("task", Some(""))]);
        assert_eq!(derive_placeholders("{x:a:b}"), vec![var("x", Some("a:b"))], "first colon splits");
        assert_eq!(derive_placeholders("{{task}}"), vec![], "escape — no variable");
        assert_eq!(derive_placeholders(r#"{"a": 1}"#), vec![], "invalid name");
        assert_eq!(derive_placeholders("{my var}"), vec![], "space");
        assert_eq!(derive_placeholders("{x-1_Y}"), vec![var("x-1_Y", None)]);
        assert_eq!(derive_placeholders("{:x}"), vec![], "empty name");
        assert_eq!(
            derive_placeholders("{{{task}}}"),
            vec![var("task", None)],
            "escape + variable + escape"
        );
        assert_eq!(
            derive_placeholders("{x:a} {x:b}"),
            vec![var("x", Some("a"))],
            "rule 5: first occurrence's default wins"
        );
    }

    #[test]
    fn duplicate_names_dedupe_in_first_appearance_order() {
        assert_eq!(
            derive_placeholders("{b} {a} {b} {c}"),
            vec![var("b", None), var("a", None), var("c", None)]
        );
    }

    /// Rule 5 read plainly: the first occurrence wins even when it carries
    /// NO default — `{x} {x:b}` is `x` with no default. Flagged to the lead
    /// as a derived (not vectored) consequence so the TS lane pins the same.
    #[test]
    fn first_occurrence_without_default_wins_over_later_default() {
        assert_eq!(derive_placeholders("{x} {x:b}"), vec![var("x", None)]);
    }

    #[test]
    fn names_are_ascii_only_and_case_sensitive() {
        assert_eq!(derive_placeholders("{tickét}"), vec![], "unicode letter is prose");
        assert_eq!(
            derive_placeholders("{Task} {task}"),
            vec![var("Task", None), var("task", None)],
            "case-sensitive: two distinct variables"
        );
    }

    #[test]
    fn dots_are_no_longer_name_characters() {
        // The retired {{token}} grammar allowed dots; the v2 name class
        // [A-Za-z0-9_-] does not — `{a.b}` must be prose now.
        assert_eq!(derive_placeholders("{a.b}"), vec![]);
    }

    #[test]
    fn unclosed_and_nested_runs_are_prose() {
        assert_eq!(derive_placeholders("{task"), vec![]);
        assert_eq!(derive_placeholders("{task:unclosed"), vec![]);
        // `{a:` is invalid (its default would contain a brace) and stays
        // verbatim — but scanning resumes INSIDE the failed run, so the
        // well-formed inner `{b}` is a variable. This is the contract's
        // equivalent-token-regex semantics (a regex scan lands on `{b}`),
        // the same shape as the inner variable in the `{{{task}}}` vector.
        assert_eq!(derive_placeholders("{a:{b}}"), vec![var("b", None)]);
    }

    #[test]
    fn default_may_span_lines() {
        // The contract's token class for a default is [^{}]* — newlines are
        // not excluded, and the TS regex reads the same way.
        assert_eq!(
            derive_placeholders("{x:two\nlines}"),
            vec![var("x", Some("two\nlines"))]
        );
    }

    #[test]
    fn variable_adjacent_to_prose_braces() {
        // A prose brace must not swallow a following real variable.
        assert_eq!(derive_placeholders("{ } {task}"), vec![var("task", None)]);
        assert_eq!(derive_placeholders(r#"{"json": true} {env}"#), vec![var("env", None)]);
    }
}
