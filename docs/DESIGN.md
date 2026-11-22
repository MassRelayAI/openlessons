# openlessons: design

As of 2026-11-23. Slices 1 to 3 are built; what is not built is marked **later**.

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
| `proposal/1` | content offered in one file; `openlessons apply` places it | a file in `proposals/` until applied |
| `reviewers/1` | `REVIEWERS.json`: who may review what | one file |
| `review/1` | one entry of a record's `provenance.reviews` | inside its record |

**Standards are references, never copies.** A framework records what may be quoted, and a gate enforces it:

| `terms.quote` | we may store | used for |
|---|---|---|
| `official_text` | the publisher's wording, with its notice | open licences (England's national curriculum, OGL v3.0) |
| `ids_and_titles` | identifiers and official titles; statements in our own words | attribution licences (Common Core: its public licence and notice) |
| `ids_only` | identifiers and a link; our own label and paraphrase | everything proprietary or unread (CEFR descriptors, Cambridge, HSK, YCT, Kurikulum Merdeka) |

Terms nobody has read (`reviewed: null`) are `ids_only` by rule. Word lists and exam items of proprietary frameworks are never
stored.

**Skills are ours; alignments are claims.** `exact` means the skill is the objective. `partial` means it covers part.
`related` makes no coverage claim. An `exact` alignment may be `proposed`, as a claim waiting for a person. It reaches
`reviewed` only through a listed `subject` reviewer, and `export` leaves it out while it is below the bar. Coverage figures
keep reviewed `exact`, proposed `exact` and `partial` apart, and a consumer must never add them together.

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

**Heard prompts.** A prompt is `shown` (the default) or `heard`. A heard prompt is for learners who cannot read yet: its
`text` is the script of a recording and is never printed, so "Tap the word cat" is a fair question. It must name the
recording (`audio.asset`), and that recording must carry its licence and an attribution. A learner export and the MCP
server leave the script out.

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
  repository. The base comparison already reads only the files a change touches. **Later**: the other gates do the same.

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
| `item.no-answer-leak` | a shown prompt, or a hint, that gives the right answer and none of the wrong ones |
| `item.heard-prompt-recorded` | a heard prompt without a recorded audio asset that has a licence and an attribution |
| `asset.licensed` | an asset without a record, with a licence off the allowed list, or without attribution |
| `content.no-pii` | an email address, phone number, web address or account handle |
| `content.child-safe` | a term on the safety lexicon of the lesson's language; a language with no lexicon |
| `content.no-duplicates` | the same item twice, by id or by what it asks and accepts |
| `loop.review-before-reviewed` | `reviewed` or `published` without a passing review by a person who is not the maker; an `exact` alignment settled without a `subject` reviewer |
| `loop.reviewer-listed` | a settling review by someone `REVIEWERS.json` does not list for that role, language, subject or framework, or dated outside their term |
| `loop.base-respected` | against a base revision (`--base`): a reviewed record changed in place, moved back, or removed |
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

- Content arrives as a pull request: files in the layout above, or one **proposal file** (`proposal/1`) that
  `openlessons apply` places. Whatever a proposal file says, every record it yields is `proposed` and carries the
  proposal's maker: a proposal cannot review itself, and it cannot overwrite a reviewed record.
- Every record says who made it: `provenance.origin` is `human`, `ai` or `generator`, with the maker's public handle and,
  for AI, the model. An AI-made record that hides its origin is grounds for removal.
- The gates decide whether a proposal may be looked at. They never decide that it is good.
- `reviewed` needs a review entry (`review/1`: who, as a person or an AI, in which role, with which verdict, when) by a
  person other than the maker, and that person must be listed in [`REVIEWERS.json`](../REVIEWERS.json) for the role and for
  the record's language, subject and framework, on a date inside their term. Roles: subject, language, safety, licence.
  **The list ships empty.** Naming reviewers is the project owner's decision; until someone is listed, nothing can be
  `reviewed`. A change to the list is a change to who is trusted: it is made on its own, never together with content. There
  is no `CODEOWNERS` file yet because no public reviewer team exists to name (see CONTRIBUTING).
- **A change is judged against its base** (`validate --base origin/main`, run by CI on every pull request). A change may add
  anything and may change anything unreviewed. A `reviewed` or `published` record may only move forward with reviews
  appended, or be reopened: back to `proposed` with a higher `version` (skill) or `revision` (lesson, path). An item is
  content-addressed, so it is never edited: a new item names the old one in `supersedes`, and only then may the old one go.
- Every graded item says how its answer was verified (`key.verification.method`: `constructed`, `computed`, `human`,
  `solver`). An answer only a model produced is confirmed in review; arithmetic is computed, not guessed.
- A consumer chooses its own bar. The default for anything shown to children is `published` only. The whole seed is
  `proposed`: it was drafted with AI and no educator has reviewed it yet.

## 5. Adoption by any harness

- **No code needed.** Read `index/catalog.json`, then `index/<locale>/<subject>.jsonl`, then the lesson folder.
  [`SKILL.md`](../SKILL.md) tells an agent how to find, use and propose lessons.
- **The CLI** (`openlessons`, Rust): `validate`, `selftest`, `index`, `find`, `export`, `import`, `apply`, `observe`, `stamp`,
  `id`. Rust because the tool must read untrusted contributions in CI with no install scripts, walk a very large tree fast,
  and ship as one binary; it matches the sibling tools.
- **`export`** writes one course in three shapes, and takes nothing below `published` unless told to (`--min-status`):

  | format | holds | for |
  |---|---|---|
  | `bundle` | every record of the course, as stored | a server, an agent |
  | `learner` | lessons and item `view`s only: no key, hint, feedback or provenance | a learner's device |
  | `pack` (`pack/1`) | one path file with objectives, units, lessons and stored items (pool, difficulty, solution, answer groups), and a keyed strings file for the course's language | a lesson player that loads keyed packs |

  A pack keeps what its shape has no place for (status, provenance, verification) under `ext`, so `import` reads it back
  into the same records. The round trip (export, import, compare) is a test over every course in the tree. The player's
  own lines (`feedback.correct`, `feedback.try_again`) are referenced by key and not shipped: they are interface text,
  not curriculum.
- **`observe`** prints `observation/1` records: one per gate, and one per standards level and locale with the skills,
  lessons, items, reviewed and proposed alignments and objectives reached. The rules that score them are in
  [`scoregraph/`](../scoregraph); a score ledger ingests both and gates a change that lowers the score.
- **The MCP server** (`openlessons mcp`, standard input and output) gives an agent four tools: `find_lessons`,
  `get_lesson` (the learner's view: no key, no hints, no script of a heard prompt; refused below `min_status`, default
  `published`), `validate_proposal` (the gates over a proposal, nothing saved) and `propose_lesson`. The server is
  read-only unless started with `--allow-write`, and then it writes one thing: a proposal file under `proposals/`, for a
  maintainer to apply and a listed person to review. It cannot write a lesson or a review.

## 6. Decisions

1. Skills are the canonical unit; standards and lessons both point at skills, never at each other.
2. Proprietary frameworks are referenced by identifier and link only. No exception without written terms on record.
3. The answer never shares an object with what the learner sees.
4. Lessons are written per language. There is no shared string table.
5. Lesson and item ids are hashes; skills and standards keep readable ids because people align against them.
6. A gate without a red fixture is not a gate, and the validator refuses to run on unproven gates.
7. AI may draft and propose. Only a person moves a record to `reviewed`.
8. The index is generated and committed, so a harness with no tools can still search.

## 7. Next

1. Reviewers named in `REVIEWERS.json` by the project owner, then the first `reviewed` content.
2. Recordings for the seed's audio cues, and heard prompts for the listening items the leak gate turned away.
3. Objective-level nodes for the rest of the open frameworks; the gates on changed shards only.
4. `CODEOWNERS` once public reviewer teams exist; an HTTP transport for the MCP server if a host needs one.
