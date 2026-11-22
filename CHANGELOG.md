# Changelog

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
