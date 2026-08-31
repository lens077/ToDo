# Lossless / Faithful Round-Trip Markdown Parsing in Rust

Research for: a kanban TODO app where `TODO.md` is the source of truth and must be
written back with all untouched bytes preserved exactly.

Everything marked **[VERIFIED-EXPERIMENT]** was executed locally with
cargo 1.97.1 against the exact crate versions in the lockfile below.
Test project: `/tmp/mdrt` (pulldown-cmark, comrak, markdown-rs, markdown-it) and
`/tmp/pan` (panache-parser).

Exact versions tested:

| crate | version |
|---|---|
| pulldown-cmark | 0.13.4 |
| pulldown-cmark-to-cmark | 22.0.1 |
| comrak | 0.54.0 |
| markdown (markdown-rs) | 1.0.0 |
| markdown-it | 0.6.1 |
| panache-parser | 0.28.0 |

## The torture fixture

A 726-byte `TODO.md` containing deliberately "hostile" formatting: YAML frontmatter,
an HTML comment, `*`/`+`/`-` list markers mixed, `_emphasis_` and `__strong__`,
4-space *and* TAB nested indentation, trailing double-space hard breaks, extra
spaces after a checkbox, `1.` and `2)` ordered styles, a link reference definition,
a raw HTML block, and a GFM table.

## Headline result

| Approach | Byte-identical round trip? |
|---|---|
| pulldown-cmark + pulldown-cmark-to-cmark `cmark()` | ❌ 726 → 702 bytes |
| pulldown-cmark + `cmark_with_source_range()` | ❌ 726 → 702 bytes |
| comrak `format_commonmark` | ❌ 726 → 727 bytes |
| markdown-rs | ❌ no serializer exists at all |
| **panache-parser (rowan CST)** | **✅ 726 → 726, identical** |
| **Surgical byte-range splice (pulldown-cmark offsets)** | **✅ identical** |

## 1. pulldown-cmark

**Byte offsets: YES.** [VERIFIED — docs.rs]
`Parser::into_offset_iter()` returns `OffsetIter`, whose
`type Item = (Event<'a>, Range<usize>)` — genuine **byte** offsets, not line/col.
<https://docs.rs/pulldown-cmark/0.13.4/pulldown_cmark/struct.OffsetIter.html>

`pulldown-cmark-to-cmark` 22.0.1 (2026-08-10, Apache-2.0, Byron) is the companion
writer. It does have source-range-aware entry points
(`cmark_with_source_range`, `cmark_resume_with_source_range_and_options`) documented as
"preserving the escape characters in `source`".
<https://docs.rs/pulldown-cmark-to-cmark/22.0.1/pulldown_cmark_to_cmark/>

**Crucially, the README never claims losslessness** — only that it supports everything
pulldown-cmark supports. <https://github.com/Byron/pulldown-cmark-to-cmark>

**[VERIFIED-EXPERIMENT]** Both `cmark()` and `cmark_with_source_range()` produced
identical, lossy output on the fixture:

- `_milk_` → `*milk*` (emphasis char normalized)
- `__parser__` → `**parser**` (strong char normalized)
- `+ [ ]` and `- [ ]` → `* [ ]` (all list markers normalized)
- 4-space and TAB nesting → 2-space nesting (indentation normalized)
- `- [ ]   Extra spaces` → `* [ ] Extra spaces` (internal spacing collapsed)
- one trailing space of the hard break dropped

Verdict: **excellent for positions, unusable as a faithful writer.**

## 2. comrak

**Positions: LINE/COLUMN only, not byte offsets.** [VERIFIED — source]
`comrak::nodes::Ast` has `pub sourcepos: Sourcepos`; `Sourcepos { start: LineColumn, end: LineColumn }`
and `LineColumn { line: usize, column: usize }` (`comrak-0.54.0/src/nodes.rs:800,841,895`).

Precision caveats, from maintainer-acknowledged issues:

- **`column` is a UTF-8 *byte* count, not a character count.** Issue #495 "sourcepos is
  Unicode-naïve": *"`columns` is a byte count, not characters/graphemes/whateverthehep or columns."*
  <https://github.com/kivikakk/comrak/issues/495>
- Ranges are **end-inclusive**, and a proposal to add char-based columns was filed as
  #777. <https://github.com/kivikakk/comrak/issues/777>
- A long tail of sourcepos correctness bugs has been filed and fixed (#503 inline code,
  #562 escaped spans, #567 nested blockquote, #572 links, #591 tables, #696 html blocks,
  #703 lists, #745 table cells). Accuracy has improved a lot but the history shows
  sourcepos is subtle, especially for inlines.

**`format_commonmark` normalizes heavily.** [VERIFIED-EXPERIMENT] On the fixture:

- `* [ ]` / `+ [ ]` → `- [ ]` (marker normalized)
- `_milk_` → `*milk*`, `__parser__` → `**parser**`
- trailing-space hard break → backslash (`\`) hard break
- **injected a synthetic `<!-- end list -->` comment** into the document
- **inlined the link reference definition**: `[spec][spec-ref]` became
  `[spec](https://example.com/spec "The Spec")` — the `[spec-ref]: ...` definition is
  destroyed as a *reference*

Maintainer-confirmed round-trip defects: #685 code-fence info string gains a leading
space (<https://github.com/kivikakk/comrak/issues/685>), #520 spurious backslash
escaping and `___` → `-----` (<https://github.com/kivikakk/comrak/issues/520>),
#551 `**Hello&#32;**` fails to round trip — *and upstream `cmark` behaves the same*
(<https://github.com/kivikakk/comrak/issues/551>).

Maintenance: **healthy.** 0.54.0 released 2026-07-12, pushed 2026-08-10, 13 open issues,
7.26M downloads.

Verdict: **well maintained, good line/col positions, but its writer is a normalizer, not a preserver.**

## 3. markdown-rs (wooorm/markdown-rs)

**This is the decisive finding: there is NO serializer.** [VERIFIED — source]
The entire public API of `markdown` 1.0.0 is:

```
pub fn to_html(value: &str) -> String
pub fn to_html_with_options(value: &str, options: &Options) -> Result<String, Message>
pub fn to_mdast(value: &str, options: &ParseOptions) -> Result<Node, Message>
```

`grep to_markdown` over `src/lib.rs` returns nothing; `src/` contains `to_html.rs` and
`to_mdast.rs` and no writer. mdast nodes *do* carry positions
(**[VERIFIED-EXPERIMENT]** `to_mdast` succeeded and root `position()` was `Some`).

A separate crate `mdast_util_to_markdown` exists but is **0.0.2, published 2025-04-23,
with no repository field on crates.io** — pre-alpha.

**Maintenance: effectively dormant.** [VERIFIED — gh api]
- last commit: **2025-04-23** (the `1.0.0` release commit)
- `pushed_at`: 2025-04-23
- **38 open issues**

So: markdown-rs shipped 1.0.0 and has had no commits for well over a year.

Verdict: **parse-only, dormant. Not a candidate for write-back.**

## 4. Lossless / CST candidates

### panache-parser — the real find ⭐

`panache-parser` 0.28.0 — "Lossless CST parser and syntax wrappers for Pandoc markdown,
Quarto, and RMarkdown", <https://github.com/jolars/panache> (MIT, 215★, pushed 2026-08-28).

It is built on **`rowan` 0.17** — the same lossless CST library that powers rust-analyzer.
That is the correct architecture for this problem.

**[VERIFIED-EXPERIMENT]** `panache_parser::parse(&src, None).text().to_string() == src` →
**`true`, 726 → 726 bytes, byte-identical.** It exposes byte ranges via
`text_range()` and models exactly the things other parsers drop:

```
YAML_METADATA    0..54
BLANK_LINE       54..55
HTML_BLOCK_RAW   55..101
HEADING          102..109
```

It has a dedicated `TASK_CHECKBOX` syntax kind (`src/syntax/kind.rs:37`).

**Honest caveats [VERIFIED-EXPERIMENT]:**
- It found **8** `TASK_CHECKBOX` tokens where pulldown-cmark found **9** — it misses
  `2) [x] ...`, i.e. the `)`-delimited ordered list item. It targets **Pandoc** dialect,
  not GFM.
- In one probe a checkbox was classified `UNRESOLVED_REFERENCE`, so kind assignment is
  not fully reliable yet.
- **Adoption risk is real**: ~1,971 downloads, 42 versions, repo created 2025-09-03,
  version 0.28.0 — pre-1.0 and moving fast.
- It conflicts with comrak on the `entities` dependency (had to be tested in isolation).

### markdown-it (markdown-it-rust)

**Source maps: YES, byte offsets.** [VERIFIED — source]
`Node` has `pub srcmap: Option<SourcePos>` (`src/parser/node.rs:21`) and
`SourcePos::get_byte_offsets(&self) -> (usize, usize)` (`src/common/sourcemap.rs:83`).
No markdown writer. Last release **0.6.1, 2024-07-07** — quiet.

### jotdown

Djot parser, 0.10.0, active (pushed 2026-08-13). It *does* have an `OffsetIter` and a
`Render` trait. **But Djot is not Markdown** — it is a different language by the
CommonMark author. Parsing a `TODO.md` with it would misinterpret the file. **Not relevant**
unless you are willing to change the file format itself.

### autosurgeon — NOT MARKDOWN

Plainly: `autosurgeon` is **not a markdown library**. It is
"A library for working with data in automerge documents" —
Automerge **CRDT** tooling (<https://github.com/automerge/autosurgeon>). Irrelevant here,
*except* conceptually: if you ever want concurrent edits, that is its problem domain.

### tree-sitter-markdown

<https://github.com/tree-sitter-grammars/tree-sitter-markdown> (621★, pushed 2026-07-19,
72 open issues). tree-sitter is genuinely a **CST** with byte ranges
(`start_byte`/`end_byte`) and is designed for incremental editing. It is a legitimate
alternative for the "positions" half of the job. Costs: a C dependency + grammar bindings,
markdown is split into two grammars (block + inline) which complicates queries, and there
is **no markdown writer** — you would still splice text yourself. I did **not**
empirically test the Rust bindings here (**NOT VERIFIED**).

### levy

"Lossless markdown parser and emitter" — but **0.1.0, published 2021-12-13, single
version, no repository**. Abandoned. Do not use.

### twig-doc

3.2.1, "Parse, query, edit, and losslessly round-trip Djot, Markdown, HTML, and XML".
Active (2026-08-26) but only 1,227 downloads. **NOT VERIFIED** — I did not test it.

## 5. How other ecosystems solve this

They mostly **don't preserve — they normalize on purpose.**

- **mdformat (Python)** describes itself as *"an opinionated Markdown formatter that can
  be used to enforce a consistent style"*, and it *"often resorts to backslash escaping
  these characters to ensure its formatting changes never alter a rendered document."*
  Note the guarantee: it preserves the **rendered output**, not the source bytes.
  <https://mdformat.readthedocs.io/en/latest/>
- **Prettier** normalizes list markers and emphasis by design, and the team
  **explicitly rejected** an option to control the emphasis character (the "fewer options"
  philosophy). <https://github.com/prettier/prettier/discussions/18741>. Its markdown
  support PR spells out the normalization table: `emphasis: _`, `strong: **`,
  `listItem: -`. <https://github.com/prettier/prettier/pull/2943>
- **remark-stringify** likewise has settings (`emphasis: '*'`) — i.e. it *picks* a style
  rather than remembering yours.

**Conclusion this forces:** "parse to AST → serialize" is essentially **never** byte-lossless,
in any ecosystem. An AST deliberately discards the syntactic choices (which marker, which
emphasis char, how much whitespace) that you are required to preserve. A *CST* keeps them;
an *AST* cannot. This is a categorical result, not a Rust limitation.

## 6. The pragmatic strategy: surgical byte-range splicing

**This works, and I proved it.**

### Toggling checkboxes — easy and exact

**[VERIFIED-EXPERIMENT]** Using `into_offset_iter()`, every `Event::TaskListMarker`
carries a byte range that is *exactly* the three bytes `[ ]` / `[x]`:

```
checked=false range=124..127 src="[ ]"
checked=true  range=434..437 src="[x]"
```

Toggling all 9 markers back-to-front, then toggling them all back, reproduced the
original file **byte-identically (726 bytes)** — frontmatter, HTML comment, TABs,
trailing spaces, `_`/`__`, `+` markers and the link reference definition all intact.

It is also robust where it matters **[VERIFIED-EXPERIMENT]**:
- **CRLF**: `"# T\r\n\r\n- [ ] alpha\r\n"` → toggled correctly, CRLF preserved.
- **No final newline**: `"- [ ] no final newline"` → marker range correct.
- **Unicode**: `"- [ ] 学习 Rust"` / `émoji 🎉` → byte ranges land on char boundaries,
  61 → 61 bytes, still valid UTF-8.
- **Extra spaces**: `-   [x]  marker offset` → range is exactly `[x]`, not the spaces.

### Editing a title — easy

`Event::Text` ranges are precise: for `- [ ] Buy _milk_ and eggs` you get
`Text("Buy ") 6..10`, `Text("milk") 11..15`, `Text(" and eggs") 16..25`.
Replace the text spans and the `_..._` delimiters survive untouched.

### Moving a task between columns — genuinely hard, and here is the trap

**[VERIFIED-EXPERIMENT] My first attempt silently corrupted the document.** Naively
expanding `Tag::Item` ranges to whole lines gave:

```
"+ [ ] Item with a plus marker\n\n## In Progress\n"
```

The item's range **swallowed the following blank line and the next `## In Progress`
heading**, so "moving one card" also teleported a column header. Result: untouched
lines were *not* preserved.

Two structural hazards, both confirmed:
1. **Item ranges include trailing blank lines** and can run into the next block.
2. **Item ranges nest and overlap** — `263..381` (parent) contains `298..341` (child),
   and one child range even *began* with `\n`.

**The fix that works [VERIFIED-EXPERIMENT]:** compute a *clamped block* per task —
start at the beginning of the marker's own line; end at whichever comes first: the next
task marker's line start, or the next structural boundary (`Tag::Heading` / `Event::Rule`);
then walk backwards over trailing blank lines. That yields clean blocks:

```
"+ [ ] Item with a plus marker\n"
"    - [ ] nested subtask with 4-space indent\n"
```

Re-running the move with clamped blocks: **all untouched lines identical = true**,
**line multiset preserved = true**.

Remaining known gap: the *last* task in the document has no following boundary, so its
block over-extends to EOF — you must also clamp on "next block-level element", not just
headings.

### Pitfalls, consolidated

1. **Offset invalidation** — apply edits **back-to-front** (descending `range.start`), or
   re-parse between edits. My move applied a delete + an insert as one sorted,
   non-overlapping edit set applied in reverse. Confirmed necessary.
2. **Overlapping ranges** — nested list items nest. Never apply two edits whose ranges
   overlap; assert non-overlap before applying.
3. **Indentation on move** — moving a nested item to top level (or vice versa) requires
   re-indenting the moved block. Byte splicing will *not* do this for you.
4. **Marker style on move** — I moved a `+ [ ]` item into a `1.`/`2)` ordered section and
   got `+ [ ]` sitting under `2)`. Preserving bytes is not the same as preserving
   *validity*. Decide policy: keep the origin marker, or adopt the destination list's.
5. **CRLF / final newline** — never `lines()`-and-`join("\n")` the whole file; that
   silently rewrites CRLF and can drop or add a trailing newline. Splice the original
   buffer instead.
6. **Blank-line ownership** — a card's "block" must not eat the blank line separating it
   from the next heading.

## Verdict table

| Library | Positions? | Serializer? | Lossless round trip? | Maintained? |
|---|---|---|---|---|
| **pulldown-cmark** 0.13.4 | ✅ **byte** `Range<usize>` via `into_offset_iter()` | ✗ (separate crate) | n/a (parser) | ✅ 2026-05-20, 142M dl |
| pulldown-cmark-to-cmark 22.0.1 | consumes ranges | ✅ | ❌ **VERIFIED lossy** (`_`→`*`, markers→`*`, indent, spacing) | ✅ 2026-08-10 |
| **comrak** 0.54.0 | ⚠️ **line/col only**, column = UTF-8 *bytes*, end-inclusive | ✅ `format_commonmark` | ❌ **VERIFIED lossy** (+ injects `<!-- end list -->`, inlines refdefs) | ✅ 2026-07-12, 7.2M dl |
| **markdown-rs** 1.0.0 | ✅ mdast positions | ❌ **none exists** | ❌ impossible | ❌ **dormant, last commit 2025-04-23, 38 open issues** |
| markdown-it 0.6.1 | ✅ **byte** `srcmap.get_byte_offsets()` | ❌ | n/a | ⚠️ quiet since 2024-07 |
| **panache-parser** 0.28.0 | ✅ **byte** `text_range()`, rowan CST | ✅ `node.text()` | ✅ **VERIFIED byte-identical** | ✅ 2026-08-28 but **pre-1.0, ~2k dl** |
| jotdown 0.10.0 | ✅ OffsetIter | ✅ Render | n/a | ✅ — **but Djot, not Markdown** |
| tree-sitter-markdown | ✅ byte ranges, true CST | ❌ | n/a | ✅ 2026-07-19 — **NOT independently verified** |
| levy 0.1.0 | ? | claims emitter | ? | ❌ **abandoned 2021, 1 version** |
| autosurgeon | — | — | — | **NOT MARKDOWN — Automerge CRDT** |

## Recommendation

**Use pulldown-cmark 0.13 for positions, and never re-serialize. Splice the original byte
buffer.**

Rationale: it is the only option that combines (a) true **byte** offsets, (b) GFM task-list
support that correctly handled all 9 checkboxes in my fixture including `2)` ordered items,
and (c) overwhelming maturity (142M downloads, actively released). The splice approach was
empirically proven byte-identical.

### Architecture

```
TODO.md (bytes, the single source of truth)
    │
    ├─ read to String, remember: had_bom, line_ending (CRLF/LF), had_final_newline
    │
    ├─ INDEX  (pulldown-cmark into_offset_iter, read-only)
    │     TaskId -> { marker_range, title_range, block_range(clamped), depth, section }
    │     Model is a *view*. It never owns the text.
    │
    ├─ EDIT   produce Vec<(Range<usize>, String)>
    │     assert: no two ranges overlap
    │     sort by range.start, apply in REVERSE
    │
    └─ WRITE  atomically (write temp + rename), preserving trailing-newline policy
```

Concretely:

- **Toggle** → replace `marker_range` with `"[x]"`/`"[ ]"`. Exact, 3 bytes, proven safe.
- **Edit title** → replace the `Text` span(s) inside the item, leaving delimiters alone.
- **Move / reorder** → cut the *clamped* block, insert at a computed line boundary, as one
  reverse-applied edit set. Re-indent explicitly if depth changes; decide a marker policy.
- **Re-parse after every write** rather than trying to patch offsets incrementally. A
  TODO.md is kilobytes; pulldown-cmark is extremely fast. Do not optimize this.

### On panache-parser

It is architecturally the *right* answer (rowan CST, verified byte-identical, models
frontmatter and blank lines explicitly) and worth watching or prototyping against. I am
not recommending it as the primary dependency today because it is pre-1.0 with ~2k
downloads, targets the **Pandoc** dialect, and **[VERIFIED-EXPERIMENT]** missed the `2) [x]`
task in my fixture. If your TODO.md is Pandoc-flavored and you can pin the version, it
becomes very attractive — and you get "unrecognized syntax survives" for free rather than
by discipline.

### How to verify fidelity (do all four)

1. **Identity property test** — parse + apply *zero* edits ⇒ output must be
   `assert_eq!(bytes_in, bytes_out)`.
2. **Involution property test** — the one I ran: toggle every checkbox, toggle them all
   back, assert byte-identical. This catches marker drift beautifully.
3. **Golden files** — keep the torture fixture (frontmatter, `+`/`*`/`-`, TABs, trailing
   spaces, refdefs, raw HTML, CRLF variant, no-final-newline variant) and assert exact
   bytes after each operation.
4. **Untouched-line invariant** — after any single-card operation, assert the multiset of
   lines is preserved (this is exactly the check that caught my heading-swallowing bug),
   plus: every line not belonging to the edited card is unchanged and in the same order.

Add `proptest` generating random toggle/move sequences and assert the file always
re-parses to the same task set with the same count.

## Verified vs. inferred

**VERIFIED by running code locally** (cargo 1.97.1, versions pinned above):
- `OffsetIter::Item = (Event, Range<usize>)`, byte offsets, exact `[ ]` spans
- pulldown-cmark-to-cmark `cmark()` **and** `cmark_with_source_range()` are both lossy
- comrak `format_commonmark` is lossy, injects `<!-- end list -->`, inlines refdefs
- markdown-rs 1.0.0 exports only `to_html`/`to_html_with_options`/`to_mdast` — no writer
- markdown-it exposes `srcmap` / `get_byte_offsets`
- panache-parser round-trips the fixture byte-identically; finds 8 of 9 checkboxes
- splice-based toggle is byte-identical; CRLF, Unicode, no-final-newline all safe
- the naive move corrupts the file; the clamped-block move preserves the line multiset

**VERIFIED from primary sources** (docs.rs, GitHub API, crate source):
- comrak `Sourcepos`/`LineColumn` struct shape (`nodes.rs:800,841,895`)
- comrak issues #495, #520, #551, #685, #777
- markdown-rs last commit 2025-04-23, 38 open issues (gh api)
- release dates/versions for all crates (gh release list + crates.io API)
- mdformat and Prettier normalization stances

**INFERENCE / NOT VERIFIED — treat with caution:**
- tree-sitter-markdown's practical ergonomics from Rust — I read the repo metadata but did
  **not** build or test the bindings.
- `twig-doc` and `levy` — **not tested at all**; judged only on crates.io metadata.
- Whether comrak's sourcepos is *currently* accurate enough for surgical inline edits.
  The bug history suggests block-level sourcepos is solid and inline is riskier, but I did
  not systematically test comrak sourcepos accuracy.
- panache-parser's behaviour on GFM-specific syntax beyond my one fixture.
- My fixture is one document. It is deliberately hostile, but it is not a corpus.
  Before shipping, run the identity property test across your real TODO.md history.
