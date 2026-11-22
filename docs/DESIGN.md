# openlessons: design

As of 2026-11-23. Slice 1 is built; what is not built is marked **later**.

openlessons is a curriculum commons kept as plain files: skills, lessons and items anyone may use, aligned to public standards
frameworks by reference. Any harness (an app, an agent, a script) can read it without our code. Content is CC BY 4.0, code is
Apache-2.0. The repository is a place for a very large number of lessons, and a loop that lets people and AI add to it without
lowering its quality.

## 1. The data model

Every record is JSON with a `schema` field (`lesson/1`). The schemas are in [`schemas/`](../schemas) (JSON Schema 2020-12). A
breaking change is a new number (`lesson/2`) with a converter; a published record is never rewritten in place.

| record | what it is | key |
|---|---|---|
| `framework/1` | a standards framework, held as a **reference**, with its terms of reuse | `eng-nc-2014` |
| `standard/1` | one node of a framework: a level, a strand or an objective | `ccss-2010:math.k-1` |
| `skill/1` | one thing a learner can do, in our own words. The canonical unit: standards map onto skills | `maths.number.count-to-20` |
| `alignment/1` | skill ↔ standard, with a strength | the pair |
| `lesson/1` | one lesson in one language: what to teach, for which ages, which skills | `les_` + hash |
| `item/1` | one activity of a lesson | `itm_` + hash |
| `asset/1` | a picture or a sound, with licence and attribution | `ast_` + hash |
| `locale/1` | a language content is written in | `en`, `zh-Hans` |
| `path/1` | an ordered course of lessons | locale / subject / slug |

**Standards are references, never copies.** A framework records what may be quoted, and a gate enforces it:

| `terms.quote` | we may store | used for |
|---|---|---|
| `official_text` | the publisher's wording, with its notice | open licences (England's national curriculum, OGL v3.0) |
| `ids_and_titles` | identifiers and official titles; statements in our own words | attribution licences (Common Core: its public licence and notice) |
| `ids_only` | identifiers and a link; our own label and paraphrase | everything proprietary or unread (CEFR descriptors, Cambridge, HSK, YCT, Kurikulum Merdeka) |

Terms nobody has read (`reviewed: null`) are `ids_only` by rule. Word lists and exam items of proprietary frameworks are never
stored.

**Skills are ours; alignments are claims.** `exact` means the skill is the objective and needs a person's review. `partial`
means it covers part. `related` makes no coverage claim. A consumer must never add `partial` to `exact`.

**An item keeps its answer apart from what a learner sees.** `view` is everything that may reach a learner's device. `key`
holds the answer, tagged wrong answers, hints and how the answer was verified, and stays on the serving side. The schema
allows no answer field inside `view`. Seven kinds, each with its own view and key: `tap_choice`, `multi_select`, `drag_match`,
`sort_order`, `tap_image`, `word_tiles`, `story_choice` (ungraded). An order is judged by what the cards show, so two cards
may show the same word.

**Content is authored per language, not translated by key.** A lesson has one `locale`; its text is inline, as ICU
MessageFormat strings. The same lesson in another language is another lesson (`translationOf`), because early reading does
not translate: English starts from s-a-t-p-i-n, Indonesian from syllables. Skills carry their statement per locale.

**Pictures and sounds.** A picture is a Unicode emoji (no asset needed) or an `asset`. A sound is a `cue` naming what must be
heard (`phoneme.s`) until a recorded `asset` exists. Every asset has a licence from the allowed list and its attribution.

## 2. Layout, for a very large tree

```
standards/<framework>/framework.json            standards/<framework>/nodes/*.jsonl
skills/<subject>/<strand>/<slug>.json
alignments/<framework>/<subject>.jsonl
lessons/<locale>/<subject>/<shard>/<lesson id>/lesson.json
lessons/<locale>/<subject>/<shard>/<lesson id>/items.jsonl      one item per line, sorted by id
assets/<shard>/<asset id>.json  + the file
paths/<locale>/<subject>/<slug>.json
locales/<tag>.json
index/<locale>/<subject>.jsonl  index/catalog.json              generated, never edited
gates/rules.json  gates/data/  gates/fixtures/                  the gates' data and their red/green fixtures
```

- **Sharded by locale, then subject, then the first two hex digits of the id.** A contributor touches one small folder;
  a consumer fetches one locale and subject. Standards and alignments are sharded by framework.
- **Ids are derived, never chosen.** A lesson id is the hash of its identity (locale, subject, slug), so adding items never
  moves it. An item id is the hash of its canonical `kind`, `view` and `key`: the same item has the same id everywhere,
  and an edit is a new item. An asset id is the hash of its bytes. Canonical form: UTF-8, keys sorted, no white space, no
  floating-point numbers (positions are integers per thousand).
- **One lesson per folder, one item per line.** Items are sorted by id, so two pull requests that add items to one lesson
  rarely conflict.
- **The index is generated** by `openlessons index` from data that passes the gates. A hand edit turns a gate red.
- At true scale the tree splits by locale into several repositories with the same layout; nothing in the format assumes one
  repository. **Later**: validating only the shards a change touches.

## 3. Quality gates

Each gate is a rule with a red fixture and the shared green fixture ([`gates/`](../gates)). `openlessons validate` first proves
every gate on its fixtures (red must fail, green must pass) and refuses to judge the data otherwise: a check never seen to
fail is not known to work. All gates block.

| gate | refuses |
|---|---|
| `schema.valid` | a record that does not match its schema; malformed ICU text |
| `layout.content-addressed` | an id that is not the hash of its content; a record in the wrong place; stray files |
| `refs.resolve` | a reference to a skill, standard, locale, lesson or misconception that does not exist; a prerequisite loop |
| `standards.terms-respected` | official wording or titles beyond what the framework's terms allow |
| `lesson.aligned` | a lesson with a skill that is aligned to no standard |
| `lesson.reading-level` | text too long for the youngest stated age (words per sentence, letters per word; characters for CJK) |
| `item.answer-verified` | an answer that names nothing, two choices that look the same, a check a guess passes, no verification method |
| `item.no-answer-leak` | a prompt or hint that shows the right answer and none of the wrong ones |
| `asset.licensed` | an asset without a record, with a licence off the allowed list, or without attribution |
| `content.no-pii` | an email address, phone number, web address or account handle |
| `content.child-safe` | a term on the safety lexicon of the lesson's language; a language with no lexicon |
| `content.no-duplicates` | the same item twice, by id or by what it asks and accepts |
| `loop.review-before-reviewed` | `reviewed` or `published` without a passing review by a person who is not the maker |
| `index.fresh` | an index that is not the one the data generates |

The gates are bounds, not judgement. Reading level is a ceiling, not a readability score; the lexicon is a starter list; the
leak gate reads written text only. What they cannot see is what reviewers are for. The seed proved them useful: of 4,508
exported items, 330 were refused (184 duplicates, 146 prompts that gave the answer away) and left out.

## 4. The contribution loop

```
draft ──► proposed ──► reviewed ──► published
 author     gates green    a person who is not      a maintainer
 or AI      (pull request) the maker passes it      releases it
```

- Content arrives as a pull request that adds files in the layout above (**later**: a single proposal file an agent can post).
- Every record says who made it: `provenance.origin` is `human`, `ai` or `generator`, with the maker's public handle and,
  for AI, the model. An AI-made record that hides its origin is grounds for removal.
- The gates decide whether a proposal may be looked at. They never decide that it is good.
- `reviewed` needs a review entry by a person other than the maker (roles: subject, language, safety, licence). The gate
  checks the record; that the reviewer is a real person is enforced by review of the pull request (**later**: CODEOWNERS
  per locale and subject).
- Every graded item says how its answer was verified (`key.verification.method`: `constructed`, `computed`, `human`,
  `solver`). An answer only a model produced is confirmed in review; arithmetic is computed, not guessed.
- A consumer chooses its own bar. The default for anything shown to children is `published` only. The whole seed is
  `proposed`: it was drafted with AI and no educator has reviewed it yet.

## 5. Adoption by any harness

- **No code needed.** Read `index/catalog.json`, then `index/<locale>/<subject>.jsonl`, then the lesson folder.
  [`SKILL.md`](../SKILL.md) tells an agent how to find, use and propose lessons.
- **The CLI** (`openlessons`, Rust): `validate`, `selftest`, `index`, `find`, `stamp`, `id`. Rust because the tool must run
  untrusted contributions in CI with no install scripts, walk a very large tree fast, and ship as one binary; it matches the
  sibling tools.
- **Later**: `openlessons export` (a lesson bundle, and a learner bundle with every `key` removed, in the pack format an app
  consumes), an MCP server over the same index, `observation/1` output so a score ledger can track quality over time.

## 6. Decisions

1. Skills are the canonical unit; standards and lessons both point at skills, never at each other.
2. Proprietary frameworks are referenced by identifier and link only. No exception without written terms on record.
3. The answer never shares an object with what the learner sees.
4. Lessons are written per language. There is no shared string table.
5. Lesson and item ids are hashes; skills and standards keep readable ids because people align against them.
6. A gate without a red fixture is not a gate, and the validator refuses to run on unproven gates.
7. AI may draft and propose. Only a person moves a record to `reviewed`.
8. The index is generated and committed, so a harness with no tools can still search.

## 7. Slice 2

1. `export`: lesson and learner bundles, and the pack format for the first consuming app; round-trip test against it.
2. Proposal files and `validate --base <ref>`: only legal status moves, only changed shards.
3. Objective-level nodes for the open frameworks, then reviewed `exact` alignments.
4. Audio-first prompts (text that is spoken, not shown), so listening items stop tripping the leak gate; recorded assets.
5. `observation/1` output and a quality ledger; an MCP server; CODEOWNERS for reviewers.
