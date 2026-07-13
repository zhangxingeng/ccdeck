//! The always-on lexical matcher: fzf-style fuzzy subsequence scoring, weighted
//! name over content. Deliberately NOT BM25/tantivy — term-frequency statistics
//! earn their keep on large noisy corpora (chat search); over a few hundred
//! curated snippets, subsequence match + field weights is better-fitting and
//! dependency-free.
//!
//! There are only two fields to weight now. Keywords, tags and category are cut:
//! search matches what the user actually wrote — the name and the prompt itself
//! — and grouping is done by putting a file in a folder.

use super::store::Snippet;

/// The name is a deliberate, hand-chosen label; the content is prose that
/// happens to contain the query. The name is the stronger signal by far.
const W_NAME: f32 = 3.0;
const W_CONTENT: f32 = 1.0;

/// A snippet's lexical result. `exact` marks a full-query name hit — the fusion
/// layer gives these a hard rank floor, so an exact name match can never be
/// buried by a middling semantic score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LexScore {
    pub score: f32,
    pub exact: bool,
}

/// Score `query` against one snippet. `None` = no match. Multi-token queries use
/// AND semantics (every whitespace token must hit somewhere), scored as the mean
/// of per-token best-field scores so longer queries aren't inflated.
///
/// An empty query scores nothing here — but it no longer means "show nothing".
/// The caller answers an empty query from the usage order instead: with no query
/// there is no score to rank by, so recency is the only meaningful order.
pub fn score_snippet(query: &str, snippet: &Snippet) -> Option<LexScore> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return None;
    }
    let name = snippet.name.to_lowercase();
    let content = snippet.content.to_lowercase();

    let mut total = 0.0;
    for token in q.split_whitespace() {
        let name_s = subseq_score(token, &name) * W_NAME;
        // Content: substring only. Subsequence over long prose scatter-matches
        // almost anything, turning the content weight into noise.
        let content_s = substring_score(token, &content) * W_CONTENT;
        let best = name_s.max(content_s);
        if best <= 0.0 {
            return None; // AND semantics: a token with no home kills the match
        }
        total += best;
    }
    let token_count = q.split_whitespace().count() as f32;
    Some(LexScore { score: total / token_count, exact: name == q })
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

    fn snippet(name: &str, content: &str) -> Snippet {
        Snippet { name: name.into(), content: content.into() }
    }

    #[test]
    fn name_match_outranks_content_match() {
        let in_name = snippet("review checklist", "unrelated");
        let in_content = snippet("unrelated", "review checklist");
        let n = score_snippet("review", &in_name).unwrap();
        let c = score_snippet("review", &in_content).unwrap();
        assert!(n.score > c.score, "name weight must dominate: {} vs {}", n.score, c.score);
    }

    #[test]
    fn exact_name_hits_are_flagged() {
        let p = snippet("senior-reviewer", "body");
        assert!(score_snippet("senior-reviewer", &p).unwrap().exact);
        assert!(score_snippet("SENIOR-REVIEWER", &p).unwrap().exact, "case-insensitive");
        assert!(!score_snippet("senior", &p).unwrap().exact, "a prefix is a match, not an exact hit");
    }

    #[test]
    fn a_subfolder_path_is_searchable_because_it_is_part_of_the_name() {
        // Folders replaced tags/category, so the folder must be matchable — this
        // is what makes "grouping by mkdir" actually usable.
        let p = snippet("rust/borrow_checker", "explain lifetimes");
        assert!(score_snippet("rust", &p).is_some());
        assert!(score_snippet("rust borrow", &p).is_some());
    }

    #[test]
    fn subsequence_finds_but_ranks_below_substring() {
        let p = snippet("senior-reviewer", "");
        let scattered = score_snippet("snrev", &p).unwrap();
        let substring = score_snippet("senior", &p).unwrap();
        assert!(scattered.score > 0.0, "subsequence must still match");
        assert!(substring.score > scattered.score);
    }

    #[test]
    fn and_semantics_all_tokens_must_match() {
        let p = snippet("senior reviewer", "checks the PR");
        assert!(score_snippet("senior pr", &p).is_some(), "tokens may hit different fields");
        assert!(score_snippet("senior zebra", &p).is_none(), "one dead token kills the match");
    }

    #[test]
    fn empty_query_scores_nothing_here() {
        // Not "shows nothing" — `match_snippets` answers an empty query from the
        // usage order instead. This function simply has nothing to rank by.
        let p = snippet("anything", "b");
        assert!(score_snippet("", &p).is_none());
        assert!(score_snippet("   ", &p).is_none());
    }

    #[test]
    fn content_requires_substring_not_subsequence() {
        let p = snippet("x", "the quick brown fox jumps");
        assert!(score_snippet("quick", &p).is_some());
        assert!(
            score_snippet("tqbfj", &p).is_none(),
            "scatter-matching prose would make the content weight pure noise"
        );
    }
}
