---
name: openlessons
description: Find, use and propose lessons in the openlessons curriculum commons (skills, lessons and practice items as JSON files, aligned to standards). Use when a task needs teaching content for a learner, a lesson for a skill or a standard, practice questions with verified answers, or when adding new lessons to the commons.
---

# openlessons: how an agent finds, uses and proposes lessons

openlessons is a folder of JSON files. You need no special tool: reading files is enough. The `openlessons` CLI is faster
when it is installed (`cargo install --path crates/openlessons` in a clone of https://github.com/MassRelayAI/openlessons).

## 1. Find a lesson

Without the CLI:

1. Read `index/catalog.json`: it lists the shelves (`locale`, `subject`, `file`).
2. Read the shelf, for example `index/en/maths.jsonl`. Each line is one lesson: `id`, `title`, `ages`, `skills`, `standards`,
   `status`, `items`, `path`.
3. Open `<path>/lesson.json` and `<path>/items.jsonl`.

With the CLI (add `--json` for one JSON object per line):

```bash
openlessons find --subject maths --age 6
openlessons find --standard eng-nc-2014:y1.maths
openlessons find --skill english.phonics --locale en
openlessons find --text "count back" --status proposed
```

To go from a standard to lessons by hand: `alignments/<framework>/<subject>.jsonl` maps skills to standards; a lesson lists
its `skills`. An alignment's `strength` is `exact`, `partial` or `related`. Only `exact` means the standard is covered.

## 2. Use a lesson with a learner

To hand a course to a device, export it without answers: `openlessons export <locale>/<subject>/<slug> --format learner
--out <dir>` (add `--min-status proposed` only when an adult has agreed to unreviewed content). `--format pack` gives an
app pack with keyed strings and answer groups for a lesson player; `--format bundle` gives every record in one file.

- `lesson.json` has `teach.text` (say this first), `teach.examples`, and `teach.reteach` (say this after a wrong answer).
- Each line of `items.jsonl` is one activity. **Show the learner `view` only. Never show or send `key`**: it holds the
  answer, the hints and the feedback.
- Judge an answer against `key.answer`. For `sort_order` and `word_tiles`, compare what the cards show, not their ids.
- If the learner's wrong answer matches a `key.distractors` entry, give that entry's `feedback`: it addresses a known
  misconception. Otherwise give a hint from `key.hints`, in order. Never reveal the answer as a hint.
- `role` says what an item is for: `guided` and `practice` while learning, `check` to decide mastery (two forms, `a` and
  `b`, so a retry is not the same questions), `unit_test`, `path_test`, `placement`, `exam`. Do not use check or test items
  for practice.
- `band` is `easy`, `medium` or `hard`. Start easy.
- A skill's `mastery` (for example 4 of 5 on one check) says when to move on; `prerequisites` say what comes first.
- `audio.cue` names a sound the learner must hear (`phoneme.s`). Say it or play it; do not print it.
- Check `status`. With children, use `published` content only, unless an adult responsible for the learner has chosen
  otherwise. `proposed` means no educator has reviewed it.
- Credit the source where the lesson is shown: the lesson's `attribution` and `licence` (CC BY 4.0).

## 3. Propose a lesson

The short way: write one proposal file (`schemas/proposal.schema.json`: your `provenance`, then `skills`, `alignments` and
`lessons` with their `items`, without ids, status or schema fields) and run `openlessons apply <file>`. It computes ids,
places every record, and marks all of it `proposed`. Then do steps 6 and 7 below. The long way, file by file:

1. Pick the skill in `skills/`. If it does not exist, add `skills/<subject>/<strand>/<slug>.json` and at least one row in
   `alignments/<framework>/<subject>.jsonl`. Write statements in your own words. Never copy a standard's wording unless
   that framework's `terms.quote` is `official_text`.
2. Get the lesson's id and folder: `openlessons id <locale> <subject> <slug>`.
3. Write `lesson.json` and `items.jsonl` there (schemas: `schemas/lesson.schema.json`, `schemas/item.schema.json`; copy the
   shape of an existing lesson). Write in the lesson's language; do not translate another lesson word for word.
4. Say what you are: `"provenance": {"origin": "ai", "maker": "<your agent name>", "model": "<model>", "created": "<date>"}`
   and `"status": "proposed"`. Never set `reviewed` or `published` yourself, and never add a review entry for your own work.
5. For every graded item set `key.verification.method`: `computed` when a program worked the answer out, `solver` when a
   second model answered it without seeing the key, `human` when a person did. Do not guess arithmetic: compute it.
6. Run `openlessons stamp <lesson folder>` (writes item ids), `openlessons index`, then
   `openlessons validate --base origin/main`. Fix every finding. Add freely; never edit a `reviewed` or `published` record
   in place: revise an item with a new item that names the old one in `supersedes`, and reopen a lesson or skill by
   raising its `revision` or `version` and setting it back to `proposed`. Do not edit `index/` by hand and do not change a gate or its fixtures to make your content pass.
7. Open a pull request. A person reviews it.

What the gates will refuse: a prompt that contains the answer, two choices that look the same, text too long for the stated
age, anything on the safety lexicon, personal data (emails, phone numbers, links, handles), a duplicate of an existing item,
an asset without a licence and attribution.
