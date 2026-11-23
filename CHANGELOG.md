# Changelog

## Unreleased

## 0.5.2 (2026-11-23)

- **12 original early-reading stories, 6 in Arabic and 6 in Indonesian.** They are written for openlessons by MassRelay
  and released under CC BY 4.0. None is a translation, and none is adapted from another book. No adoptable story exists
  in either language: GDL's Arabic books are NC-SA, and its Indonesian CC BY books are Let's Read titles, still held.
- Each story is a `story_choice` item, with one or two questions about it (32 items). The stories are mapped to three
  objectives:
  - `literacy.reading.how-books-work`, for ages 4-5;
  - `literacy.reading.retell-a-story`, for ages 5-6;
  - `literacy.reading.predict`, for ages 6-8; these stories branch at a "what happens next?" page.
- Names, foods and places fit each locale. There are two courses, `paths/<ar|id>/literacy/first-stories`. Everything is
  `draft`, for the jury.
- The three objectives gain Arabic and Indonesian statements, in our own words. They are back at `draft`.

## 0.5.1 (2026-11-23)

- **The sourced AI jury (decision K4).** Lessons and items become `reviewed` only by a jury decision record,
  `review/decisions/<record id>.json` (schema `jury-decision/1`), under `review/jury-policy.md` (policy version 1; jury
  `jury-v1` in `REVIEWERS.json`, owner MassRelay). Three independent roles (subject, safety, language), unanimous to pass,
  every correctness claim citing a registry entry and a resolving source, the models recorded, 5% of passes drawn for an
  optional human spot-check. New gate `review.jury-decision` (proven red first on its fixture and in `tests/jury.rs`).
  New commands `openlessons jury orders` and `openlessons jury run [--dry-run]`; new MCP tools `jury_order` and
  `jury_submit`. Review entries take `kind: jury` with a `decision` path. Files beyond the review lane: `scan.rs`,
  `main.rs`, `mcp.rs`, `lib.rs`, the review-entry block of every record schema, `reviewers.schema.json`,
  `gates/rules.json`, the green fixture, `scoregraph/rules/quality.review.jury-decision.json`.
- **29 first-reading stories adopted** from the Global Digital Library (level 1, read without an account): 10 English, 9
  Hindi, 6 Chinese (Simplified), 4 Spanish. They were first published by StoryWeaver (18), African Storybook (9) and
  Book Dash (2), and each is one `story_choice` item. There are four courses, `paths/<locale>/literacy/open-stories`. All
  are `draft`. Five books were refused at intake: four for reading level, one with no author.
- **GDL adapter.** It reads the public book record and the unpacked EPUB (spine order). The licence is the item's own,
  twice over: the record's `license` term and every licence the book's credits page names, in English, Spanish or
  Chinese (知识共享：署名). The book's statement wins. If the two disagree in family, or if the book names two licences or one
  that cannot be read, the book is refused.
- **A distributor's copy never gets round its publisher's class.** A book first published by a source held as
  reference-only or share-alike is refused whatever the distributor's field says. This is why the Let's Read titles on
  GDL stay out.
- **Story lessons.** An adopted lesson that tells a story counts the story as its adopted wording. Its opening line is
  ours, so `content.no-copied-text` no longer treats one opening line shared by many stories as copied.
- **Sources.**
  - Bloom Library moves to `reference-only`: its licence page names only NC licences, and its book API needs a key.
  - Global Digital Library and African Storybook gain the licences actually seen, and GDL gains its importer.
  - Decided, and recorded in the sources and the README: share-alike material (Wikibooks, UNESCO) stays out of this
    repository, and a separate pack is made only if a product needs it.
- **Back to `draft` for the jury.** These skill records have statements we wrote ourselves:
  - `maths.number.count-to-10`, `maths.calc.add-within-10` and `maths.calc.subtract-within-10` (Spanish);
  - `literacy.reading.retell-a-story`, which now also has Spanish, Hindi and Chinese.

## 0.5.0 (2026-11-23)

- **Open material (OER) intake.** `sources/` registers 16 publishers of open material (`source/1`), each with the
  licences it publishes under, where an item states its own licence, the page its terms were read on (quotation, date)
  and its class: 10 `adopt` (IM K-12 Math first edition, StoryWeaver, African Storybook, Global Digital Library,
  OpenStax, Siyavula, Project Gutenberg, Book Dash, Wikimedia Commons, Bloom Library), 4 `reference-only` (Khan Academy,
  CK-12, Eureka Math / EngageNY, Let's Read) and 2 `sa-layer` (Wikibooks, UNESCO open access), kept out of the CC BY core.
  `openlessons sources` lists them.
- **`openlessons adopt <plan>`** reads the items of an adoption plan (`adopt-plan/1`) with the source's adapter, takes
  each item's OWN licence (never the site's), refuses NonCommercial, NoDerivatives, ShareAlike, unknown and unstated
  licences and any item without its credit, builds `draft` lessons mapped onto objectives and age bands, and takes out
  again any lesson a gate refuses. Adapters: IM K-12 Math lesson pages, and StoryWeaver's story record and reader pages.
- **Imported:** a K-2 slice of IM K-5 Math, six lessons (three in English, three in Spanish) with 36 practice items, as
  two courses `paths/<locale>/maths/im-k2-slice`. No StoryWeaver story yet: their page text is served to signed-in
  readers only.
- `lesson/1` gains `adopted` (source, item, title, sourceUrl, licence, licenceUrl, authors, attribution, changes,
  retrieved). Exports carry it: the learner bundle, the app pack (`ext.adopted`, restored by `import`), and a `credits`
  list in both bundles. The index line of an adopted lesson names its `source`; the catalog counts `sources`.
- **New gate `content.source-licensed`** (twenty-two gates): a registered source whose class follows from its
  licences; an item licence that is CC BY, CC0 or public domain, that the source publishes under, with the matching deed,
  its authors, attribution and an original on the source's host; a lesson credit naming the first author. Proven red
  first (`crates/openlessons/tests/adopt.rs`, `gates/fixtures/red/content.source-licensed/`).
- **`content.no-copied-text`** now allows adopted wording only where its `adopted` credit travels with it (a run of
  eight words, fourteen characters for Chinese, anywhere else is refused), lets the credit carry the publisher's © and
  notice, and refuses any text or credit that names a NonCommercial or NoDerivatives licence (`licenceMarks` in
  `gates/data/protected.json`).
- `gates/data/licences.json` gains `text`: the adoptable and share-alike licences and the deed of each.

## 0.4.2 (2026-11-23)

- 0.4.1's tag points at a commit whose catalogs were stale; use 0.4.2. No content changed.
- Files the 0.4.1 gate work touched beyond its lane: `schemas/skill.schema.json` (optional `deprecated`),
  `scoregraph/rules/quality.graph.ids-stable.json`, `crates/openlessons/src/observe.rs`, `crates/openlessons/src/main.rs`
  (the new gate is `unmeasured` without `--base`), `scoregraph/README.md`, `gates/README.md`, `README.md`.

## 0.4.1 (2026-11-23)

- **Objective ids are a public contract.** The new gate `graph.ids-stable` (judged with `--base`, like `loop.base-respected`)
  refuses a tree that drops or renames an objective id present on the base. A retired objective stays and says what
  replaces it: `"deprecated": {"replacedBy": "<id>", "reason": "..."}` (new optional field of `skill/1`); the replacement must
  exist. Proven red first on `gates/fixtures/graph.ids-stable/`.
- `index/catalog.json` carries the release `version`.

- **The copied-text guard covers ten frameworks.** `gates/data/protected/<framework>.txt` holds fingerprints (hashes of
  runs of eight words, fourteen characters for Chinese) of the official texts of Common Core (mathematics and English
  language arts), the CEFR Companion volume, PISA 2025, Hong Kong's mathematics curriculum guide, South Africa's CAPS
  (mathematics 4-6, life skills R-3), Taiwan's guidelines (general, English), the CSTA PK-K standards, the ISTE Standards
  for Students, the CASEL framework and the Cambridge Young Learners handbook: 320,588 fingerprints, no text. Each list
  was proven red first: a passage of its source, planted in a copy of the tree, passed every gate without the list and
  was refused with it (`gates/data/protected.json` records the sources and the proofs).
- `framework/1` gains `terms.guard`: `fingerprints` or `paraphrase_rule`. The 22 held-back frameworks with no official
  text that could be fetched say so, with the reason; the gate `standards.terms-respected` refuses a record that claims
  a list it does not have.
- `openlessons fingerprint` reads a whole text as one run of words, takes `--window` and `--sample`, and prints a list
  file.

## 0.4.0 (2026-11-23)

The content loop: where the benchmarks come from, who a lesson is for, what is missing, and a safe way to add it.

- **Standards registry.** `framework/1` gains `subjects`, `span` (ages and the publisher's own grades), `languages` and
  `terms.evidence` (a short quotation from the terms page). 38 frameworks, each with the licence finding and the page it
  was read on: 6 whose wording may be quoted (England's national curriculum and early years framework, the Australian
  Curriculum, Scotland's Curriculum for Excellence, Spain's primary decree, Japan's Course of Study), 3 held to identifiers
  and titles (Common Core, Taiwan's guidelines, PISA 2025), 29 by identifier only. `openlessons registry` lists them.
- **Age bands.** `bands/age-bands.json` (`age-bands/1`): ten bands from 0-2 to adult, each mapped to US grades, England's
  key stages and years, Indonesia's kelas and fase, Australia, India, Singapore, China, Spain, CEFR and ISCED. Standards
  nodes name their bands (`standard/1` gains `bands`).
- **Topic graph.** `topics/<subject>.json` (`subject/1`): ten subjects, 86 topics. `skill/1` gains `topic`, `bands` and
  `locales`, all 49 existing skills are placed, and 132 objectives are added across maths, literacy, science, computing,
  the arts, wellbeing and life skills, with 474 `related` alignments. `index/objectives.jsonl` lists every objective with
  its standards and the languages it is taught in.
- **Four more languages of instruction**: Spanish, Hindi, Arabic and Chinese (Traditional), each with a locale record and
  a starter safety lexicon. Words in Devanagari and vocalised Arabic are counted whole.
- **Three gates** (twenty in all): `graph.placed`, `alignment.band-matches` and `content.no-copied-text`, which refuses
  the marks of copied material and any run of six words from a text whose terms forbid copying it
  (`gates/data/protected.json` holds hashes, never the text; `openlessons fingerprint` adds to it).
- **Gap report.** `openlessons gaps`: objective x age band x language cells with no lesson or too few items, ranked by
  demand from the registry and the alignments, then from usage counts under `signals/` (`signal/1`: counts of at least 20
  per cell and week, with no field for a learner).
- **The loop.** `openlessons loop pick` writes work orders (`order/1`); `loop land` judges proposal files with every gate,
  places what passes at status `draft`, and writes a run report (`run/1`) on a branch or a pull request; `loop run` does
  both around a drafter command. A proposal that came through the loop cannot add a skill or an alignment, must answer an
  order of its run, and leaves no trace when a gate refuses it.
- **MCP**: `gap_report` and `get_work_orders`, both read-only.
- A scheduled workflow (`gaps`) publishes the gap report weekly. It uses no model and no secret.
- Fixed: the paraphrase of one Common Core standard ran eleven words alongside the publisher's sentence; the new gate
  found it and it was rewritten.

## 0.3.0 (2026-11-23)

- `openlessons mcp`: a Model Context Protocol server on standard input and output with four tools: `find_lessons`,
  `get_lesson` (learner view, never an answer), `validate_proposal`, `propose_lesson`. Read-only unless `--allow-write`,
  and then it saves proposal files under `proposals/` only.
- Heard prompts (`prompt.mode: heard`) and the gate `item.heard-prompt-recorded`: the script is never printed, and the
  prompt must name a recorded audio asset with a licence and an attribution.
- `REVIEWERS.json` (`reviewers/1`), the review entry schema (`review/1`) and the gate `loop.reviewer-listed`: a record is
  `reviewed` only through a person listed for that role, language, subject and framework. The list ships empty.
- Restored, in the form it should have had: an `exact` alignment may be `proposed`, but it is settled only by a `subject`
  reviewer, and `export` leaves it out below the bar. 0.2.0 had dropped the rule instead of fixing it.
- `proposals/` is checked by the gates: every file in it must be a well-formed proposal.

## 0.2.0 (2026-11-23)

- `export`: a course as a bundle, a learner bundle with every answer removed, or an app pack (`pack/1`: keyed strings,
  objectives, stored items with answer groups). Nothing below `published` leaves unless `--min-status` says so.
- `import`: an app pack back into records. Export then import is tested on all four seed courses (49 lessons, 4,178 items).
- Proposal files (`proposal/1`) and `apply`: one file in, every record placed and marked `proposed`.
- `validate --base <ref>` and the gate `loop.base-respected`: a reviewed record moves forward, is reopened with a higher
  version, or is revised by a new record; it never changes silently. Items gain `supersedes`; lessons and paths `revision`.
- Objective-level standards: 15 statements of England's Year 1 curriculum (official wording, OGL v3.0) and 5 Common Core
  standards (identifiers, official domain titles, our own wording), with 9 strand nodes; 18 new alignments, 4 of them
  `exact`, all `proposed` until a person reviews them.
- `observe`: gate results and coverage per framework, level and locale as `observation/1`, with a ruleset under
  `scoregraph/` that a score ledger ingests.
- An `exact` alignment may now be proposed; it counts as coverage only once reviewed.

## 0.1.0 (2026-11-23)

First slice.

- The data model as JSON Schemas: `framework/1`, `standard/1`, `skill/1`, `alignment/1`, `lesson/1`, `item/1`, `asset/1`,
  `locale/1`, `path/1`.
- The layout: sharded by locale, subject and id; content-derived ids; a generated index.
- Fourteen quality gates, each proven on a red and a green fixture before every run.
- The CLI: `validate`, `selftest`, `index`, `find`, `stamp`, `id`.
- Seed, all at status `proposed` (drafted with AI, not yet reviewed by an educator): 49 skills, 49 lessons and 4,178 items in
  four courses (first English, early maths, first Mandarin, first reading in Indonesian); 8 frameworks as references with
  10 level nodes and 67 `partial` alignments.
- The gates refused 330 of the 4,508 items first exported, and they were left out: 184 duplicates (the same question under
  another role) and 146 prompts that showed the answer.
