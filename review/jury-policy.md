# The jury policy

**Policy version 1.** Jury id `jury-v1` in `REVIEWERS.json`. Accountable owner: **MassRelay** (the organisation).

A lesson or an item becomes `reviewed` in one way only: a sourced AI jury decides it, and the decision is kept in
`review/decisions/<record id>.json`. The gate `review.jury-decision` refuses a `reviewed` or `published` lesson or item
without that file, and refuses the file when its verdicts are not unanimous, when a correctness claim is unsourced, or when
a source does not resolve. Skills, alignments and paths are still settled by the people listed in `REVIEWERS.json`.

## Three roles, decided apart

| role | asks | judges |
|---|---|---|
| `subject` | Is it right? | correctness of the teaching, the answers, the hints and the feedback, against cited sources |
| `safety` | Is it safe and right for this age? | child safety and age fit |
| `language` | Is it good language here? | language and culture for the record's own locale |

**Independence.** Each role is filled by a separate reviewer: a separate model session, started fresh, that is given the
work order for its role and nothing else. A work order (`openlessons jury orders`) holds the content, its objective, its
aligned standards and the role's checklist; it never holds another role's verdict, and no role is shown one before it has
given its own. A verdict is submitted once per role and per lesson and is not revised after the others are known. The
harness records how it kept the roles apart in the decision's `independence` field, and the model of every role in
`roles.<role>.model`. Nobody reviews their own work: the model session that drafted a lesson never sits on its jury.

**Unanimity.** A record passes only when all three roles pass. Any objection keeps it a `draft`, and the objection is
written into the decision's `notes`, prefixed with its role, so the next draft can fix it. A lesson leaves draft only when
the lesson and every one of its items pass.

## Sources

Every correctness claim the `subject` role makes names:

1. **a registry entry**: a standard id from `standards/` (`ccss-2010:K.OA.A.1`) that is aligned to a skill of the record,
   itself or through an ancestor, and
2. **a real reference**: either a web address (`https://...`) with the date it was read (`accessed`, not after the
   decision date), or a source cited by its licence: public domain, `CC0-1.0`, or `CC-BY-*`.

A claim without both fails, and so does a subject verdict with no claim at all: an unsourced pass is not a pass. Reviewers
cite what they read; a reference that was not opened is not cited.

## Checklists

Each verdict answers every check of its role with true or false. A false check is an objection, with a note.

### subject

- `objective-taught`: the lesson teaches the objective it names, at the level of its age band.
- `answer-correct`: every graded item has exactly one correct answer, the key names it, and no distractor is also right.
- `explanation-correct`: the teaching text, examples, hints and feedback state nothing false or misleading.
- `misconceptions-sound`: each distractor and its feedback answer a real misconception, and say so correctly.
- `hints-fair`: a hint helps thinking without giving the answer away (no position, colour, or elimination give-aways).
- `claims-sourced`: every correctness claim cites a registry entry and a real reference, as above.

### safety

- `no-harm`: nothing violent, sexual, frightening, hateful, or dangerous to imitate.
- `no-personal-data`: never asks a child for personal information, and never encourages contact with strangers.
- `safe-advice`: any safety guidance (online or physical) is accurate and points the child to a trusted adult.
- `age-fit`: ideas, words and sentence length fit the youngest age the lesson names.
- `no-manipulation`: no pressure, shame, fear, rewards for secrecy, persuasive design, brands or advertising.
- `inclusive`: no stereotype or exclusion by sex, ethnicity, religion, disability, family form or wealth.

### language (for the record's own locale)

- `natural`: reads as a native speaker of the locale would write for a child; not a literal translation.
- `correct`: grammar, spelling, script and punctuation are right for the locale (for example Traditional, not
  Simplified, characters in `zh-Hant`).
- `register`: words and tone are right for the age.
- `culture`: names, foods, places, measures and customs fit the locale or are neutral; nothing offends there.
- `consistent`: the prompt, the options, the answer and the teaching text use the same terms.

## The decision record

`review/decisions/<record id>.json` (schema `schemas/jury-decision.schema.json`) keeps: the record and its lesson, the
`contentHash` of exactly the content decided (sha256 of the canonical record without `status` and `provenance`; the
decision lapses when the content changes), the jury, the policy and its version, the date, the harness and its
independence statement, each role's verdict, model, checks, claims with sources and notes, the outcome, and the spot-check
mark. A passing record carries a review entry
`{"by": "jury-v1", "kind": "jury", "role": "jury", "verdict": "pass", "date": ..., "decision": "review/decisions/<id>.json"}`.

## Spot-check

Of the records a run passes, 5% (rounded up) are drawn and marked `spotCheck: pending` for an optional human spot-check.
The draw is by the lowest `sha256("<policy version>:<date>:<record id>")`: it cannot be steered by the content, and anyone
can repeat it. A spot-check never blocks; a person who disagrees sets `disagreed`, which reopens the record by the usual
rules, and a rising share of `disagreed` is the signal that the jury drifts and the policy needs a new version.

## Running it

1. `openlessons jury orders --lesson <id> --out .jury/orders`: one work order per role and lesson.
2. Any harness fills each role apart (through the CLI files or the MCP tools `jury_order` and `jury_submit`), writing
   `.jury/verdicts/<role>/<lesson id>.json`.
3. `openlessons jury run --verdicts .jury/verdicts --dry-run` shows what would pass; without `--dry-run` it writes the
   decisions, sets the passing lessons and items `reviewed`, and leaves the rest `draft` with notes.

The program never calls a model itself: the harness that fills the roles brings the model and its key.

## Changing the policy

A change to the checklists or the rules is a new policy version: `policyVersion` in `REVIEWERS.json` and here move
together, and decisions made under an older version are re-run before they settle anything new.
