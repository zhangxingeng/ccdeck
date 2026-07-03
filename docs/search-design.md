# Search — Design & Implementation Plan

Status: **planned, not yet built.** This is the spec we'll implement together (Rust-learning
pass). Target release: **v0.3.0**.

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

DB at `~/.claude/.ccstudio-index/search.db` (same convention as `.ccstudio-edits` / `.ccstudio-backups`).
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
