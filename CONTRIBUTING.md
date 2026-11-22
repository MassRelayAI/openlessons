# Contributing

Lessons, skills, alignments, translations into a new language, safety-lexicon entries, gates and code are all welcome, from
people and from AI agents.

## Licences

By contributing you agree that your **content** (anything under `lessons/`, `skills/`, `alignments/`, `paths/`, `assets/`,
`locales/`) is licensed under [CC BY 4.0](LICENSE-CONTENT) and your **code** (everything else) under
[Apache-2.0](LICENSE). Contribute only what you have the right to license this way.

- Do not paste text from a textbook, a paid course, an exam paper or a word list.
- Do not copy a standard's wording unless its framework says `"quote": "official_text"`. Write the statement in your own words
  and link the source.
- A picture or a sound needs an `asset/1` record with a licence from `gates/data/licences.json` and its attribution.
  NonCommercial, NoDerivatives and ShareAlike material cannot be accepted.

## The loop

```
draft ──► proposed ──► reviewed ──► published
```

1. **Write.** Follow [SKILL.md](SKILL.md) section 3: it is the same procedure for a person and for an agent.
2. **Say who made it.** Every record has `provenance`: `origin` is `human`, `ai` or `generator`; `maker` is a public handle
   or an agent name (not an email address); for AI also `model`. Content drafted by AI and edited by you is `ai`.
3. **Pass the gates.**

   ```bash
   openlessons stamp lessons/<locale>/<subject>/<shard>/<lesson id>
   openlessons index
   openlessons validate
   cargo test          # when you changed code, a gate or a fixture
   ```

4. **Open a pull request** with status `proposed`. Keep it small: one lesson, or one skill with its lessons.
5. **Review.** A person who did not make the content reads it for the subject, the language and child safety, and adds a
   review entry. Only then does a maintainer set `reviewed`, and later `published`. Nobody reviews their own work, and an
   AI review never counts as the review.

## What a reviewer looks for (the gates cannot)

- Each item tests its skill and nothing later; the answer is right and no other answer is defensible.
- One new idea per item; hints give a step, never the answer.
- The language is natural for a child of that age in that place; names, food and money fit the region.
- Nothing frightens, shames or sells; feedback never blames the learner.
- Usable by ear alone where the learner cannot read yet.

## Changing a gate

A gate is a rule in `gates/rules.json`, its code, and a red fixture under `gates/fixtures/red/<rule>/` that it must fail.
A new gate arrives red-first: add the fixture, see `openlessons selftest` fail, then write the check. Loosening a gate
needs a reason in the pull request and a maintainer's agreement; never loosen a gate in the same change as the content it
would let through.

## Conduct

Be kind and specific. This is material for children: when in doubt about safety, leave it out and ask.
