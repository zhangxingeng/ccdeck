//! The always-on lexical matcher: fzf-style fuzzy subsequence scoring with
//! field weighting (title > keywords/tags > body). Deliberately NOT
//! BM25/tantivy — term-frequency statistics earn their keep on large noisy
//! corpora (chat search); over a few hundred curated snippets, subsequence
//! match + field weights is better-fitting and dependency-free (contract).

use super::store::Piece;

const W_TITLE: f32 = 3.0;
const W_KEYWORD: f32 = 2.0;
const W_BODY: f32 = 1.0;

/// A piece's lexical result. `exact` marks a full-query title/keyword/tag hit
/// — the fusion layer gives these a hard rank floor (contract: an exact hit
/// is never buried by a middling semantic score).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LexScore {
    pub score: f32,
    pub exact: bool,
}

/// Score `query` against one piece. `None` = no match. Multi-token queries
/// use AND semantics (every whitespace token must hit somewhere), scored as
/// the mean of per-token best-field scores so longer queries aren't inflated.
pub fn score_piece(query: &str, piece: &Piece) -> Option<LexScore> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return None;
    }
    let title = piece.title.to_lowercase();
    let keywords: Vec<String> = piece
        .keywords
        .iter()
        .chain(piece.tags.iter())
        .map(|k| k.to_lowercase())
        .collect();
    let body = piece.body.to_lowercase();

    let mut total = 0.0;
    for token in q.split_whitespace() {
        let title_s = subseq_score(token, &title) * W_TITLE;
        let kw_s = keywords
            .iter()
            .map(|k| subseq_score(token, k))
            .fold(0.0f32, f32::max)
            * W_KEYWORD;
        // Body: substring only. Subsequence over long prose scatter-matches
        // almost anything, turning the body weight into noise.
        let body_s = substring_score(token, &body) * W_BODY;
        let best = title_s.max(kw_s).max(body_s);
        if best <= 0.0 {
            return None; // AND semantics: a token with no home kills the match
        }
        total += best;
    }
    let token_count = q.split_whitespace().count() as f32;
    let exact = title == q || keywords.iter().any(|k| k == &q);
    Some(LexScore { score: total / token_count, exact })
}

/// Substring-tier score: 0 if absent; 1.0 base when present, boosted for
/// matching at the start (+0.6) or a word boundary (+0.3), and for consuming
/// the whole field (+0.6). Both inputs must already be lowercase.
fn substring_score(needle: &str, hay: &str) -> f32 {
    let Some(pos) = hay.find(needle) else {
        return 0.0;
    };
    let mut s = 1.0;
    if pos == 0 {
        s += 0.6;
    } else if !hay[..pos].chars().next_back().unwrap_or('a').is_alphanumeric() {
        s += 0.3;
    }
    if hay.len() == needle.len() {
        s += 0.6;
    }
    s
}

/// Full fuzzy tier: substring score when present, else an in-order
/// subsequence match scored 0..0.5 by compactness (total gap between matched
/// chars) — "snrev" still finds "senior-reviewer", but scattered matches rank
/// well below any substring hit.
fn subseq_score(needle: &str, hay: &str) -> f32 {
    let substring = substring_score(needle, hay);
    if substring > 0.0 {
        return substring;
    }
    let mut gaps: usize = 0;
    let mut started = false;
    let mut pending_gap: usize = 0;
    let mut hay_chars = hay.chars();
    for nc in needle.chars() {
        let mut found = false;
        for hc in hay_chars.by_ref() {
            if hc == nc {
                if started {
                    gaps += pending_gap;
                }
                started = true;
                pending_gap = 0;
                found = true;
                break;
            }
            if started {
                pending_gap += 1;
            }
        }
        if !found {
            return 0.0;
        }
    }
    let len = needle.chars().count();
    0.5 * (len as f32 / (len + gaps) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts::store::Scope;

    fn piece(title: &str, keywords: &[&str], body: &str) -> Piece {
        Piece {
            id: "t".into(),
            title: title.into(),
            body: body.into(),
            keywords: keywords.iter().map(|s| s.to_string()).collect(),
            tags: vec![],
            category: None,
            scope: Scope::Global,
            placeholders: vec![],
            created_at: 0,
            updated_at: 0,
            versions: vec![],
            extra: serde_json::Map::new(),
        }
    }

    #[test]
    fn title_match_outranks_body_match() {
        let in_title = piece("review checklist", &[], "unrelated");
        let in_body = piece("unrelated", &[], "review checklist");
        let t = score_piece("review", &in_title).unwrap();
        let b = score_piece("review", &in_body).unwrap();
        assert!(t.score > b.score, "title weight must dominate: {} vs {}", t.score, b.score);
    }

    #[test]
    fn keyword_match_outranks_body_match() {
        let in_kw = piece("unrelated", &["review"], "nothing");
        let in_body = piece("unrelated", &[], "review here");
        assert!(score_piece("review", &in_kw).unwrap().score > score_piece("review", &in_body).unwrap().score);
    }

    #[test]
    fn exact_title_and_keyword_hits_are_flagged() {
        let p = piece("senior-reviewer", &["role"], "body");
        assert!(score_piece("senior-reviewer", &p).unwrap().exact);
        assert!(score_piece("SENIOR-REVIEWER", &p).unwrap().exact, "case-insensitive");
        assert!(score_piece("role", &p).unwrap().exact, "keyword equality is exact too");
        assert!(!score_piece("senior", &p).unwrap().exact, "prefix is a match, not an exact hit");
    }

    #[test]
    fn subsequence_finds_but_ranks_below_substring() {
        let p = piece("senior-reviewer", &[], "");
        let scattered = score_piece("snrev", &p).unwrap();
        let substring = score_piece("senior", &p).unwrap();
        assert!(scattered.score > 0.0, "subsequence must still match");
        assert!(substring.score > scattered.score);
    }

    #[test]
    fn and_semantics_all_tokens_must_match() {
        let p = piece("senior reviewer", &[], "checks the PR");
        assert!(score_piece("senior pr", &p).is_some(), "tokens may hit different fields");
        assert!(score_piece("senior zebra", &p).is_none(), "one dead token kills the match");
    }

    #[test]
    fn empty_query_matches_nothing() {
        let p = piece("anything", &[], "b");
        assert!(score_piece("", &p).is_none());
        assert!(score_piece("   ", &p).is_none());
    }

    #[test]
    fn body_requires_substring_not_subsequence() {
        let p = piece("x", &[], "the quick brown fox jumps");
        assert!(score_piece("quick", &p).is_some());
        assert!(
            score_piece("tqbfj", &p).is_none(),
            "scatter-matching prose would make body weight pure noise"
        );
    }
}
