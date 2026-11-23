# Contributing

Lessons, skills, alignments, translations into a new language, safety-lexicon entries, gates and code are all welcome, from
people and from AI agents.

## Licences

By contributing you agree that your **content** (anything under `lessons/`, `skills/`, `alignments/`, `paths/`, `assets/`,
`locales/`) is licensed under [CC BY 4.0](LICENSE-CONTENT) and your **code** (everything else) under
[Apache-2.0](LICENSE). Contribute only what you have the right to license this way.

- Do not paste text from a textbook, a paid course, an exam paper or a word list. The gate `content.no-copied-text`
  catches the careless copy; a licence reviewer catches the rest.
- Do not copy a standard's wording unless its framework says `"quote": "official_text"`. Write the statement in your own words
  and link the source.
- A picture or a sound needs an `asset/1` record with a licence from `gates/data/licences.json` and its attribution.
  NonCommercial, NoDerivatives and ShareAlike material cannot be accepted.

## The loop

```
draft ──► proposed ──► reviewed ──► published
```

`draft` is where the content loop lands what a model drafted and the gates passed (`openlessons loop land`; see the
README, "Run the loop"). `proposed` is a contribution a person or an agent offers by pull request or `openlessons apply`.
Neither has been read by an educator, and neither may reach a child. From either, the only way up is a review by a
listed person.

A new objective, topic or alignment changes the topic graph: it comes as its own pull request, never through the loop.
An objective names its `topic` (listed in `topics/<subject>.json`) and its age `bands` (`bands/age-bands.json`), and a
prerequisite never starts later than the objective that needs it. Start a new alignment as `related` unless you have
read the standard: `partial` and `exact` are claims about coverage, and they must share an age band with the standard.

A new framework comes with its terms: the licence, the page where you read it, a short quotation from that page and the
day you read it. If you could not read the terms, say so (`"reviewed": null`): the framework is then held by identifier
only. Say also how our wording is guarded against its text (`terms.guard`): a fingerprint list (see `gates/README.md`),
or the paraphrase rule alone.

1. **Write.** Follow [SKILL.md](SKILL.md) section 3: it is the same procedure for a person and for an agent.
2. **Say who made it.** Every record has `provenance`: `origin` is `human`, `ai` or `generator`; `maker` is a public handle
   or an agent name (not an email address); for AI also `model`. Content drafted by AI and edited by you is `ai`.
3. **Pass the gates.**

   ```bash
   openlessons stamp lessons/<locale>/<subject>/<shard>/<lesson id>
   openlessons index
   openlessons validate --base origin/main
   cargo test          # when you changed code, a gate or a fixture
   ```

4. **Open a pull request** with status `proposed`. Keep it small: one lesson, or one skill with its lessons. An agent can
   instead write one proposal file and run `openlessons apply <file>`, which places the records and marks them `proposed`.
5. **Review.** A person who did not make the content, and who is listed in `REVIEWERS.json` for that role, language and
   subject, reads it and adds a review entry. Only then does a maintainer set `reviewed`, and later `published`. Nobody
   reviews their own work, and an AI review never counts as the review.

## What a reviewer looks for (the gates cannot)

- Each item tests its skill and nothing later; the answer is right and no other answer is defensible.
- One new idea per item; hints give a step, never the answer.
- The language is natural for a child of that age in that place; names, food and money fit the region.
- Nothing frightens, shames or sells; feedback never blames the learner.
- Usable by ear alone where the learner cannot read yet.

## What a change may do to reviewed content

Add anything. Change anything that is `draft` or `proposed`. A record that is `reviewed` or `published` on `main` may only
move forward (reviews appended, nothing else touched), or be reopened: back to `proposed`, with a higher `version` (skill)
or `revision` (lesson, path). A reviewed item is never edited: a new item names it in `supersedes`, and only then may the
old one go. `openlessons validate --base origin/main` enforces this, and CI runs it on every pull request.

## Reviewers

`REVIEWERS.json` lists the people whose review can settle a record: a public handle, the roles they review in (subject,
language, safety, licence), the languages, subjects and frameworks they cover (`*` for all), and the dates of their term.
A gate refuses any `reviewed` or `published` record whose passing review is not from someone listed for exactly that. An
`exact` alignment needs a `subject` reviewer listed for its framework.

**The list is empty.** Who reviews lessons for children is the project owner's decision, and it has not been made yet.
Until it is, everything stays `proposed`, however good it looks. Do not add a reviewer in a pull request that also adds
or changes content: a change to the list is reviewed on its own.

## CODEOWNERS

There is no `CODEOWNERS` file yet, on purpose. An owner must be a public team with write access, one per language and
subject, made of people qualified to review lessons for children. No such team exists today, and a file naming an owner
that cannot review would only pretend to guard the content. Until teams exist, a maintainer assigns the reviewer on each
pull request, and nothing in the tree is `reviewed`.

## Changing a gate

A gate is a rule in `gates/rules.json`, its code, and a red fixture under `gates/fixtures/red/<rule>/` that it must fail.
A gate is never loosened to let existing content through: if the data fails a rule, the data is what changes. A new gate
arrives red-first: add the fixture, see `openlessons selftest` fail, then write the check. Loosening a gate
needs a reason in the pull request and a maintainer's agreement; never loosen a gate in the same change as the content it
would let through.

## Conduct

Be kind and specific. This is material for children: when in doubt about safety, leave it out and ask.
