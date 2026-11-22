# Changelog

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
