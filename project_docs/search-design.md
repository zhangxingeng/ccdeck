# Search — Design & Implementation Plan

Status: **BUILT (Phase 1/2) — all 12 milestones + all of Phase 2, including keyboard nav and the
tool-name / current-session filters, as of 2026-07-04.** Backend: `src-tauri/src/search/{db,extract,
index,query,state}.rs` — 30 unit/integration tests green. Frontend: `src/lib/search.svelte.ts`
(store), consumed by `src/lib/components/BrowseView.svelte` (Browse + Search were later merged into
one view — the original standalone `SearchView.svelte` no longer exists, see
`project_docs/roadmap.md`) and `src/lib/components/InlineSearchPanel.svelte` ("find in this chat",
mounted from `SessionEditor.svelte`). This doc is otherwise historical for Phase 1/2; build-progress
items live in `project_docs/roadmap.md`.

**See "v2 — fuzzy/intent search redesign" directly below for the current, in-progress design
direction (tracked as issue #5) — it supersedes §2, §9, and parts of §12/§14 of the Phase 1/2 spec
that follows it.**

**Deviations from the Phase 1/2 spec below (intentional):**
- `blocks` gained a **`uuid`** column (historical — the `blocks` table itself is gone as of the v2
  tantivy rewrite; `uuid` lives on the tantivy schema now, see the v2 section's "Schema" subsection) —
  the frontend regroups entries into turns and flattens blocks, so raw `line_no` can't locate a hit.
  **Correction (found in the issue #5 Gate-2 audit):** jump-to-hit actually navigates by `uuid` alone
  today — `line_no`/`block_no` are only ever used as dedup-key strings, never for positioning, and
  `block_no`'s own numbering has since diverged from the frontend's (see `extract.rs`'s module doc).
- The cold tier **scans** un-cached sessions for correctness but does **not** write the cache from the
  read path (avoids write-lock contention); the background indexer warms the cache instead.
- Search opens a dedicated **read** connection (`db::open_read`, no schema DDL) so it never contends
  with the indexer's write transaction.

---

## v2 — Fuzzy/intent search redesign (backend BUILT 2026-07-07, tracked as issue #5)

Founder direction: drop exact/regex matching entirely. This tool's job is finding relevant fragments
in a large pile of information-sparse chat history — that's an intent/fuzzy-match problem, not a
precision-editing problem. Regex/whole-word/case-sensitive toggles were the right choice for a
VS-Code-style *exact* search (§2 below), but that's the wrong tool for this job: real usage needs
"find what I meant," not "find this literal string."

**What changes vs. the Phase 1/2 design above:**
- **Matching engine**: the `regex`-crate substring/regex scan (§9) is replaced by
  **[tantivy](https://github.com/quickwit-oss/tantivy)** — an embedded Lucene-style index with BM25
  relevance ranking and `FuzzyTermQuery` (Levenshtein-automaton typo tolerance). Chosen over
  hand-rolling a fuzzy matcher (a large, well-trodden problem not worth re-solving) and over
  `milli`/Meilisearch's embeddable core (better out-of-the-box relevance, but explicitly kept
  pre-1.0/unstable by its own maintainers — worth a hands-on spike later only if tantivy's ranking
  underwhelms, not the default pick). No Python/cross-language bridge needed — tantivy is pure Rust
  and covers this use case at the required ~1GB scale.
- **Storage**: tantivy owns its own on-disk index in place of the `blocks` SQLite table and the regex
  row-scan in `query.rs`. The `session_files` fingerprint table (mtime/size invalidation) is unrelated
  to the matching algorithm and stays as-is.
- **Extraction** (`extract.rs`), **indexer sweep/invalidation** (`index.rs`'s no-filesystem-watcher,
  mtime/size fingerprinting), and **streaming + cancellation** (`state.rs`'s `Channel<SearchHit>` +
  atomic generation counter) are all orthogonal to regex-vs-fuzzy and carry over unchanged in spirit —
  only their write target moves from a SQLite `INSERT` to a tantivy `IndexWriter`.
- **Frontend**: the three VS-Code-style toggle buttons (case-sensitive / whole-word / regex) are
  removed entirely — no "advanced mode" escape hatch. This does not lose exact-match capability: BM25
  + fuzzy ranking still surfaces exact/near-exact hits first, it just removes manual strictness
  control. Source/project/date-range/tool-name filters are unrelated to match mode and are kept as-is.
- **Result ordering**: changes from "session mtime desc, append-only" (§8, step 3) to genuine
  relevance ranking. This is an intentional upgrade, not a side effect to minimize — recency ordering
  was a proxy for relevance in the absence of real ranking.

**Known engineering risk — resolved as built**: tantivy's fuzzy hits score on par with exact hits by
default (open upstream issue). Fixed with a boosted union query, built per query token in
`query.rs::build_query`: `Should(BoostQuery(TermQuery(token), boost=3.0), FuzzyTermQuery(token,
distance=1, transposition=true))`, then every token's group is itself OR'd (`Should`) together and
wrapped in a top-level `BooleanQuery` `Must` alongside the filter clauses (source/tool-name/project as
`Should`-groups, `ts` as a `RangeQuery`, `session_path` as an exact `TermQuery`). The boost keeps
exact/near-exact term hits ranked above loosely-fuzzy ones per token, while more matched tokens still
accumulate more BM25 score than fewer — covered by
`query::tests::fuzzy_query_finds_typo_and_ranks_exact_above_it` (single-token) and
`query::tests::multi_token_query_ranks_both_tokens_matched_above_one_token_matched` (the multi-token
case; the single-token test alone couldn't and didn't cover this — the earlier version of this doc
overclaimed that, found in the issue #5 Gate-2 audit).

**Short-token fuzzy noise (found + fixed in the Gate-2 audit)**: `FUZZY_DISTANCE=1` applied uniformly to
every token meant a short/common token (2-3 chars) fuzzy-matched a huge fraction of any real vocabulary
— empirically, a 2-char query matched nearly every unrelated short word in a small test corpus, all
bunched at the same low score. Fixed with `query::MIN_FUZZY_TOKEN_LEN = 3`: tokens shorter than that get
exact-only matching (no fuzzy sibling clause at all), mirroring Elasticsearch's `fuzziness: "AUTO"` floor.

**Schema (as built, tokenizer updated in the Gate-2 audit)**: `session_path`/`project`/`uuid`/`source`
are `STRING | STORED` (single indexed term, exact-match filterable); `text` is tokenized + `STORED` with
its own registered tokenizer (`index::TEXT_TOKENIZER = "ccstudio_text"`, `WithFreqsAndPositions`) rather
than tantivy's built-in `"default"` — **because the built-in `"default"` applies
`RemoveLongFilter::limit(40)`, silently dropping any token 40+ characters at index time (a git SHA, a
long hash, a long identifier — routine content in Claude Code chat history) with zero error surfaced
anywhere.** Our own copy of the same pipeline (`SimpleTokenizer` → `RemoveLongFilter::limit(200)` →
`LowerCaser`) raises that ceiling to 200 — comfortably past a SHA-256 hex digest (64) or a UUID (36).
`ts` is `INDEXED | STORED | FAST`; `line_no`/`block_no` are `STORED` only. A `tool_name` field (`STRING |
STORED`, the first line of a `tool_use` block's text, `""` otherwise) was added so the tool-name filter
is an exact `TermQuery` instead of emulating the old `LIKE 'name\n%'` prefix scan.

**Migration (as built, updated in the Gate-2 audit)**: this app already ships to users with a populated
v1 (`blocks` SQLite table + regex scan) cache on disk. Silently swapping the matcher would leave their
`session_files` fingerprints pointing at files the *new* tantivy index has never seen, and the
invalidation sweep would treat those fingerprints as "already indexed" and skip re-extraction — users
would upgrade into an empty search index with nothing to trigger a rebuild. Fixed with an
`ENGINE_VERSION` marker (`db.rs`) in a new `search_meta` key/value table: on mismatch (or on a fresh v1
install with no marker at all), `db::ensure_engine_version` drops the legacy `blocks` table, clears every
`session_files` fingerprint, and wipes the tantivy index directory — forcing exactly one full rebuild
into the fresh engine on next launch. A no-op once the marker matches. Simple-and-total was chosen
deliberately over incremental migration logic, since this cache was always documented as a disposable,
fully-rebuildable-from-source-JSONL artifact. **Gate-2 audit finding closed:** the version string
`ensure_engine_version` compares is no longer just the hand-maintained `db::ENGINE_VERSION` constant —
the caller (`state.rs`) now combines it with `index::schema_fingerprint()`, a hash of the tantivy
schema's serialized field definitions (names, types, indexing options, tokenizer *name*), so most schema
changes force a rebuild automatically instead of depending on a human remembering to bump
`ENGINE_VERSION` by hand. `ENGINE_VERSION` itself still needs a manual bump only for the narrower class
the fingerprint can't see — a tokenizer *parameter* tweak (e.g. changing `MAX_TOKEN_LEN`'s value without
renaming `TEXT_TOKENIZER`) or a matching-semantics change with no schema-shape footprint at all.

**Cold-tier compromise (as built, semantics fixed in the Gate-2 audit)**: the cold tier (session files
not yet reflected in the tantivy index — first launch, or right after an engine-version rebuild) does
**not** build a throwaway tantivy index per uncached file. It falls back to a simpler case-insensitive
substring match (`query::cold_match`) over freshly-extracted blocks, reusing the old windowed-
snippet/char-boundary logic. **Gate-2 audit finding closed:** this used to require *every* query token
present (an AND) while the warm tier is OR-across-tokens by design — same query, a stricter and
inconsistent result set purely because a file hadn't been swept into the index yet. `cold_match` now
matches on *any* token present (OR/partial-credit, consistent with the warm tier's philosophy) and
returns a matched-token count used as the hit's `score` (a coarse relevance proxy, not a real BM25
recompute — cold hits still stream in file-walk order rather than being re-sorted, an accepted
simplification since the cold tier is normally empty and the background indexer catches up fast).

**Frontend contract (as built)**: the `search` Tauri command drops the `opts` parameter entirely (no
`SearchOpts`, no case/whole-word/regex fields anywhere in the Rust API) — not a geek-mode toggle, just
gone, matching the frontend directive above. `SearchHit` gained a `score: f32` field (BM25 + boost,
descending) alongside its existing fields. `SearchSummary.scanned` was relabeled from "SQL rows scanned"
to "total matching blocks found before the result-limit truncation" (via a `Count` collector run
alongside `TopDocs`) — same field name, an evolved meaning that fits the new engine. A `limit` is now
always applied (`DEFAULT_LIMIT = 500` when the caller passes `None`), since tantivy's `TopDocs` collector
needs a concrete top-k rather than the old unbounded regex-scan-and-emit; the frontend already paginates
in pages of 100, so this was never actually surfaced as unlimited.

**Fuzzy-hit legibility (found + fixed in the Gate-2 audit)**: `search_warm`'s snippet generator only
highlights a literal substring match, so a fuzzy-only hit (the query term isn't literally in the text —
e.g. "parser" matching stored "parsr") left `match_ranges` empty: zero visual explanation of why the
result surfaced, undercutting the point of a fuzzy engine being legible. Fixed with a best-effort
fallback (`query::fuzzy_highlight`) that scans the excerpt's words for one within edit distance of a
query token (only tokens at/above `MIN_FUZZY_TOKEN_LEN`, since shorter tokens never went through a fuzzy
clause) and highlights that instead.

**Indexing indicator under-firing (found + fixed in the Gate-2 audit)**: the frontend's "indexing N/M…"
indicator (`BrowseView.svelte`) requires `total_sessions > 0`, but `total_sessions` used to stay at its
zero default for the *entire* first-launch build — `Indexer::update_counts` only ran once, at the end of
`run_index()`. So the indicator never appeared during exactly the window it matters most (the first
build, however long it takes for a large corpus). Fixed backend-side: `run_index()` now also calls
`update_counts()` immediately after marking `building = true`, so `total_sessions` reflects the on-disk
file count from the very start of a build. No frontend change was needed — the existing condition was
already correct once the backend actually populated the count it depends on.

**Shared-engine question (prompt library, issue #7)**: prompt-library search has a different tradeoff
profile (small volume, quality/intent over performance) and is **not** sharing this tantivy engine —
see `project_docs/prompt-library-design.md` §F2 for its own (local-embedding-based) approach. Chat
search and prompt search are deliberately two separate engines, not one abstraction forced to fit
both.

---

Phase 1/2 spec follows (superseded in part — see the callouts above).

---

## Phase 2 — VS Code parity follow-ups (2026-07-03)

User feedback after using Phase 1: results should read like VS Code's search panel (per-file
match previews — already true), collapsible per-file groups, pagination to cut loading overhead,
and richer filtering. Status of each:

1. **Collapsible per-session groups — DONE 2026-07-03.** `SearchView.svelte`: each group header is
   now a `<button>` with a `▾`/`▸` chevron; clicking toggles a local `collapsed: Set<sessionPath>`
   `$state`, reset via `$effect` whenever `search.query` changes (new search = fresh result set).
   Purely a frontend change, no backend/store changes needed.

2. **Real pagination / backend limit — DONE 2026-07-03.** Added `limit: Option<usize>` to the
   `search` Tauri command + `search_streaming` + `scan_blocks` (cold path). Both tiers now stop
   scanning/snippet-building/sending once `summary.hits >= limit` and set `summary.truncated = true`.
   Frontend: replaced the hard `MAX_DISPLAY_HITS = 1000` client-side cap with a `PAGE_SIZE = 100`
   server-side limit; `loadMore()` bumps limit and re-runs; "Load more results…" button in
   `SearchView.svelte` when `moreAvailable`. New queries + filter changes reset `limit` to page 1.
   All 18 Rust tests green, `pnpm check` + `pnpm build` clean. Files: `query.rs`, `state.rs`,
   `api.ts`, `search.svelte.ts`, `SearchView.svelte`, `types.ts`.

3. **Keyboard navigation (↑/↓/Enter across hits) — DONE 2026-07-04.** `SearchView.svelte` tracks a
   `focusedIdx` over `visibleHits` (hits flattened from non-collapsed groups only, in display
   order). ↓/↑ on the search input move focus and scroll the focused hit into view (`scrollIntoView`
   with `block: 'nearest'`); Enter jumps to the focused hit via `onJump`. Resets to -1 on every new
   query, alongside the existing collapse-state reset.

4. **Tool-name + current-session filters — DONE 2026-07-04.** Two independent, additive filters:
   - **Tool-name filter**: `SearchFilters.toolName` (`tool_name: Option<String>` in Rust) restricts
     to `tool_use` blocks whose text is exactly the tool name or starts with `"{name}\n"` (matches
     the extraction format confirmed in `search/extract.rs` — `format!("{name}\n{}", ...)`).
     Implemented as an escaped `LIKE ... ESCAPE '\'` prefix match in `candidate_sql`, and mirrored in
     the cold-path `passes_source_and_date`. It **overrides** `sources` (replacing the `b.source IN
     (...)` clause rather than ANDing with it) since a tool-name filter only makes sense against
     `tool_use` blocks regardless of what `sources` says.
   - **Current-session-only filter**: `SearchFilters.sessionPath` (`session_path: Option<String>`)
     adds a `WHERE b.session_path = ?` clause in `candidate_sql`, plus a matching file-skip in the
     cold-path directory loop in `state.rs`. Wired through the store (`search.svelte.ts`:
     `sessionOnly` + `currentSessionPath`, set via `initSearch(currentSessionPath)`) and a "This
     session only" checkbox in `SearchView.svelte`, shown when a session path is available.
   - Explicitly **out of scope**: model/git-branch filtering (needs new columns + reindex).
   - **Known gap**: today's navigation only reaches Search from Browse, so `currentSessionPath` is
     never actually populated yet in practice — the filter is correct and unit-tested, but no UI
     entry point passes a real session path in. See `project_docs/roadmap.md` §Phase 6, item 4.

See `project_docs/roadmap.md` for what came after Search Phase 2 — the wider CC Deck rebrand, settings editor,
and terminal launcher.

## 1. Goal

VS Code-style search across all Claude Code history: fast, substring/regex, streaming results,
with filters for **source** (message / thinking / tool), **date range**, and **project** (multi-select).
Empty filters = search everything.

The guiding principle: **the user never waits.** Results stream in as they're found; each keystroke
cancels the previous search; the index is a pure accelerant that search never blocks on.

## 2. Why "like VS Code" changes the engine (key decision)

VS Code search is **not** a tokenized full-text index — it's ripgrep (Rust) doing fast
**substring/regex scans**. That's why it supports regex, whole-word, and case toggles and still
feels instant. FTS5's tokenized matching can't do arbitrary regex and has awkward
word-boundary / ≥3-char behavior.

So we do **not** tokenize. We substring/regex-match like ripgrep. SQLite stays, but its role
changes from "FTS5 inverted index" to **"extracted-text cache"**:

- The expensive part of searching JSONL is **JSON parsing**, not matching. A raw ripgrep over
  `.jsonl` would match JSON escapes, UUIDs, and metadata keys → garbage hits. We must parse each
  line and extract the clean human text first.
- So we cache the **extracted text** (one row per content block) in SQLite, invalidated by an
  mtime+size fingerprint. Search then runs the Rust `regex` engine over the cached text.
- The `regex` crate has the same SIMD literal-prefiltering that makes ripgrep fast, so this *is*
  essentially VS Code's engine, just fed pre-cleaned text.

## 3. Two-tier model (correctness never waits on the index)

- **Cold** (no cache for a session): read + parse the JSONL directly, extract, match, stream.
  Works immediately; slower. The cold scan **doubles as indexing** — it populates the cache as it
  goes, so the first search warms things up.
- **Warm** (cache present & fresh): scan the cached text — no JSON parse — much faster.
- A background task builds/refreshes the cache. Search is always correct; it just gets faster as
  the cache fills. There is no "index not ready" state the user has to think about.

## 4. Data model (SQLite, NOT FTS5)

DB at `~/.claude/.ccstudio-index/search.db` (same convention as `.ccstudio-backups`).
The DB is a **disposable cache** — always rebuildable from source JSONL — so we use aggressive,
non-durable pragmas for speed: `journal_mode=WAL`, `synchronous=OFF` during bulk build
(checkpoint at end), large transaction batches.

```sql
-- One row per session file, for invalidation.
CREATE TABLE session_files (
  session_path TEXT PRIMARY KEY,
  project      TEXT NOT NULL,   -- home-relative label (from real cwd)
  mtime        INTEGER NOT NULL,
  size         INTEGER NOT NULL,
  indexed_at   INTEGER NOT NULL
);

-- One row per extracted, searchable content block.
CREATE TABLE blocks (
  session_path TEXT NOT NULL,
  project      TEXT NOT NULL,   -- denormalized for cheap filtering
  ts           INTEGER,         -- message timestamp (epoch ms), for date filtering
  line_no      INTEGER NOT NULL,-- 0-based line index in the JSONL (for jump-to)
  block_no     INTEGER NOT NULL,-- index of the content block within that line's message
  source       TEXT NOT NULL,   -- 'user'|'assistant'|'thinking'|'tool_use'|'tool_result'
  text         TEXT NOT NULL    -- extracted plain text
);
CREATE INDEX blocks_session ON blocks(session_path);
CREATE INDEX blocks_project ON blocks(project);
CREATE INDEX blocks_ts      ON blocks(ts);
CREATE INDEX blocks_source  ON blocks(source);
```

**Source multi-select is a query-time filter** (`WHERE source IN (...)`), never a reason to
reindex — we always extract & store all sources. UI mapping:
- **Messages** (default on) → `source IN ('user','assistant')`
- **Thinking** → `source = 'thinking'`
- **Tool calls** → `source IN ('tool_use','tool_result')`

## 5. Extraction rules (what text each source yields)

Mirror `parser.ts`'s `extractContentBlocks` document-order semantics so `line_no`/`block_no` line
up with what the editor renders (needed for jump-to-hit):

- `user` / `assistant` text blocks → the text string.
- `thinking` blocks → the thinking text.
- `tool_use` → the tool name + a flattened, readable rendering of its input JSON (so searching a
  file path or command inside a tool call works).
- `tool_result` → the result's text/stdout (flattened; skip pure binary/base64).

Each extracted block records `(line_no, block_no, source, ts, text)`.

## 6. Invalidation (no filesystem watcher)

We deliberately do **not** use a `notify`-style fs watcher — this repo lives inside
`~/.claude/projects/`, and watchers here hit EMFILE (the same reason `npm run dev` is banned).
Instead:

- **Fingerprint** = `(mtime, size)` per session file in `session_files`. On a sweep, `stat()` each
  file and compare; mismatch or missing row ⇒ stale. Cheap: we only read files we're re-indexing.
- **Granularity = per session file.** Claude Code appends lines; our own Save rewrites the whole
  file. Either way the unit of change is "this file changed." Reindex = `DELETE FROM blocks WHERE
  session_path=?`, re-extract, bulk insert, upsert `session_files`.
- **Two triggers:**
  1. **Eager** — our own Save / Save-as-copy / Restore know exactly which file changed → push it
     onto an immediate reindex queue.
  2. **Lazy sweep** — on launch + every N minutes, low-priority background thread. Catches external
     changes (the CLI appending to an open session, edits outside the app).
- **Deletion cleanup** — the sweep removes `session_files` rows whose path no longer exists and
  cascades `DELETE FROM blocks`, so removed sessions stop appearing in results.

## 7. Rust concurrency (the fun part / the learning goals)

Producer/consumer: **parse in parallel, write serialized.**

- Fan session files across a bounded worker pool (rayon or N std threads). Each worker reads +
  `serde_json`-parses + extracts blocks — CPU/IO-bound, parallelizes well.
- Workers push extracted rows down an `mpsc` channel.
- **One** writer thread owns the `rusqlite` connection and does batched `INSERT`s inside big
  transactions. SQLite allows a single writer at a time — this is the canonical shape.

Rust concepts we'll hit (good learning surface): ownership across threads (`Arc`, `Send`/`Sync`),
`Result`/`?` error handling, `mpsc` channels, `rayon` parallel iterators, C FFI via `rusqlite`
(bundled SQLite), and the `regex`/`aho-corasick` crates.

## 8. Search flow

1. Frontend sends `{ query, opts: {caseSensitive, wholeWord, regex}, filters: {sources[], from,
   to, projects[]} }` plus a Tauri `Channel` handle and a fresh cancellation id.
2. Rust builds a `regex::Regex` from the query (see §9).
3. **Candidate selection, recency-ordered:** `SELECT ... FROM blocks WHERE` (source/date/project
   filters) — for fresh sessions. Order candidate **sessions by mtime desc** (most recent first),
   blocks in file order within a session, so results stream in a **stable, append-only** order (no
   mid-list reordering → no flicker).
4. For **stale/missing** sessions in the filter set, fall back to the **cold path**: read+parse
   those JSONL files directly, applying the same filters + matcher, and opportunistically populate
   the cache.
5. For each candidate block, run the regex. On match, push a hit to the channel:
   `{ session_path, project, line_no, block_no, source, ts, snippet, matchRanges }`.
6. Check the cancellation token between blocks/batches; abort promptly if superseded.

## 9. Query semantics (unify all modes through the regex crate)

- **Plain (default):** `regex::escape(query)` → literal substring match.
- **Whole word:** wrap with `\b…\b`.
- **Regex:** use the query as-is (compile errors surface as a gentle "invalid regex" hint, no
  results thrown away destructively).
- **Case:** `RegexBuilder::new(...).case_insensitive(!caseSensitive)`.

The `regex` crate auto-applies literal prefiltering (memchr/Aho-Corasick/SIMD), so even a scan over
a large text column is fast — this is what makes the "no tokenizer" approach viable.

## 10. Cancellation & ordering (anti-flicker)

- **Per-keystroke cancellation, not just debounce.** Each new query cancels the in-flight one via a
  `tokio_util::sync::CancellationToken` (or an atomic generation counter checked in the scan loop).
  Debounce alone lets a slow stale scan finish and stream results *after* a newer query started —
  the classic flicker/revert race.
- **Stable order:** candidate sessions sorted by recency up front; results only ever **append**.
- Light debounce (~80–120 ms) on top, purely to avoid launching a scan per literal keystroke.

## 11. Tauri command surface (draft)

```rust
// Streaming search. Pushes SearchHit messages onto `on_hit` as they're found.
#[tauri::command]
async fn search(
    query: String,
    opts: SearchOpts,          // { case_sensitive, whole_word, regex }
    filters: SearchFilters,    // { sources: Vec<String>, from: Option<i64>, to: Option<i64>, projects: Vec<String> }
    search_id: u64,            // for cancellation (newer id supersedes older)
    on_hit: tauri::ipc::Channel<SearchHit>,
) -> Result<SearchSummary, String>;   // summary: total hits, sessions touched, whether cache was cold

// Kick a background (re)index sweep; safe to call often (no-ops if fresh).
#[tauri::command]
async fn refresh_index() -> Result<IndexStatus, String>;

// Cheap status for a subtle "indexing… N%" indicator.
#[tauri::command]
fn index_status() -> IndexStatus;   // { total_sessions, indexed_sessions, building: bool }
```

Streaming uses **`tauri::ipc::Channel<T>`** (Tauri v2's ordered high-frequency Rust→JS primitive),
not `emit` events (which get lossy/slow at volume).

## 12. Frontend (SvelteKit)

- **New top-level Search view** (sibling of Browse), reachable from the header. (Placement TBD —
  see §14.)
- A `search.svelte.ts` store: holds query, opts, filters, and a reactive `hits` array. On query/
  filter change → debounce → bump `search_id` → open a `Channel` → call `search` → **append** each
  incoming hit to `hits`. A newer search_id makes the store ignore late hits from the old one.
- **Filter UI:** search box + three toggle buttons (Aa case, `\b` whole-word, `.*` regex, like VS
  Code); a **source multi-select** (Messages default-checked, Thinking, Tool calls); a date-range
  picker (from/to); a project multi-select (checkbox list of known projects).
- **Results:** grouped by session (session header = home-relative project + title + date), each hit
  a row with a snippet and the matched substring highlighted (`matchRanges`). Click a hit → open
  that session and **scroll to `line_no`/`block_no`** (needs a small "scroll-to-block" entry point
  added to the editor). Reuse the existing session-open path.
- **Subtle index indicator** somewhere unobtrusive ("indexing 42/120…"), driven by `index_status`.

## 13. Crates to add (src-tauri)

- `rusqlite = { version = "0.32", features = ["bundled"] }` — bundled SQLite (compiles from
  source; no system dep). We use plain tables, not FTS5.
- `regex` — the matcher.
- `rayon` — parallel parse (or hand-rolled `std::thread` pool; rayon is simpler to learn first).
- `tokio-util` — `CancellationToken` (Tauri already pulls tokio). Or roll an `AtomicU64` generation
  counter (simpler, fewer deps — decide when we build).
- (`serde_json` is already present.)

## 14. Open decisions (confirm when we build)

1. **Search view placement:** dedicated top-level view (recommended) vs. a search panel bolted into
   Browse. Leaning dedicated view.
2. **Cancellation mechanism:** `tokio_util::CancellationToken` vs. an `AtomicU64` generation counter.
   Leaning the atomic counter for simplicity + fewer deps (good first-Rust choice).
3. **rayon vs. std::thread pool** for parse fan-out. Leaning rayon (less boilerplate to learn on).
4. **Sweep interval N** and whether to sweep on every window focus vs. a timer.
5. Whether to show match **context lines** (VS Code shows the matching line; our "line" is a
   message block, so the snippet is a windowed excerpt around the match — confirm excerpt length).

## 15. Build milestones (learning-paced; do in order)

1. **Schema + rusqlite hello-world**: open/create `search.db`, create tables, a trivial insert/
   select round-trip. (Learn: `Result`, `?`, the bundled-SQLite build, borrowing a `Connection`.)
2. **Extraction in Rust**: port `extractContentBlocks` semantics; unit-test against a mock JSONL
   fixture so `line_no`/`block_no` match the frontend.
3. **Single-threaded indexer**: read a dir of sessions, extract, bulk-insert in a transaction;
   fingerprint into `session_files`. Verify counts.
4. **Invalidation sweep**: mtime/size compare, reindex-stale, delete-missing.
5. **Parallelize**: rayon parse → mpsc → single writer thread. Measure the speedup.
6. **Matcher + `search` command (warm path only)**: SQL prefilter → regex scan → return a Vec
   first (no streaming yet). Wire a minimal frontend results list.
7. **Streaming**: switch to `Channel<SearchHit>`; append in the store.
8. **Cancellation + debounce**: per-keystroke supersede; kill the flicker race.
9. **Cold-path fallback + cache-warming**: scan un-indexed sessions directly, populate as we go.
10. **Filters UI**: source multi-select, date range, project multi-select.
11. **Jump-to-hit**: scroll-to-block in the editor; highlight.
12. **Polish**: index-status indicator, empty/no-results states, regex-error hint, tests.
```
