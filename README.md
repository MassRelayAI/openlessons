# openlessons

**An open curriculum commons: skills, lessons and practice items as plain files, aligned to public standards, free for any app
or AI harness to use and to add to.**

- **Content** (everything under `lessons/`, `skills/`, `alignments/`, `paths/`, `assets/`): [CC BY 4.0](LICENSE-CONTENT).
- **Code and tools** (the CLI, schemas, gates): [Apache-2.0](LICENSE).
- **Standards** are referenced by identifier and link. They belong to their publishers; see [NOTICE](NOTICE).

> Status: early (0.3). The format, the gates, export, an MCP server and a seed are here. **The whole seed is `proposed`: it was drafted with AI and
> has not been reviewed by an educator yet.** Use `published` content for children; today there is none.

## What is in it

| | count | where |
|---|---|---|
| lessons | 49, in 4 courses: first English, early maths, first Mandarin (taught in English), first reading in Indonesian | `lessons/` |
| items (questions and activities, 7 kinds) | 4,178 | `lessons/**/items.jsonl` |
| skills, each with its misconceptions and prerequisites | 49 | `skills/` |
| standards frameworks, as references with their terms of reuse | 8 (39 nodes: 10 levels, 9 strands, 20 objectives) | `standards/` |
| alignments of skills to standards, none reviewed yet | 85 (81 `partial`, 4 `exact`, all `proposed`) | `alignments/` |
| languages | English, Indonesian; Chinese (Simplified) as a language being learned | `locales/` |

The numbers come from [`index/catalog.json`](index/catalog.json), which the tool generates.

## How it is organised

```
standards/<framework>/          a framework, its terms of reuse, its levels and objectives (references only)
skills/<subject>/<strand>/      one skill per file, in our own words
alignments/<framework>/         skill <-> standard, with a strength: exact, partial or related
lessons/<locale>/<subject>/<shard>/<lesson id>/
    lesson.json                 what to teach, for which ages, which skills
    items.jsonl                 one activity per line; `view` is what a learner sees, `key` is the answer
paths/<locale>/<subject>/       an ordered course of lessons
index/                          generated search index; never edited by hand
schemas/                        JSON Schema for every record
gates/                          the quality gates' data and their red and green fixtures
```

The design, and the reasons for it, are in [docs/DESIGN.md](docs/DESIGN.md).

## Use it

**Without any tool.** Everything is JSON. Read `index/catalog.json`, pick a shelf such as `index/en/maths.jsonl` (one lesson
per line with its ages, skills, standards and folder), then open that folder.

**With the CLI.** A recent stable Rust is needed (built and tested with 1.100).

```bash
git clone https://github.com/MassRelayAI/openlessons
cd openlessons
cargo install --path crates/openlessons

openlessons find --subject maths --age 6
openlessons find --framework ccss-2010 --json
openlessons find --skill english.phonics --text "sounds"
openlessons validate            # proves every gate on its fixtures, then checks the whole tree

# one course out: for a learner's device (no answers), or as an app pack (keyed strings, answer groups)
openlessons export en/maths/early-maths --format learner --min-status proposed --out out/
openlessons export en/maths/early-maths --format pack    --min-status proposed --out out/pack
```

`export` takes `published` content unless you lower the bar yourself with `--min-status`. An app pack reads back with
`openlessons import`; the round trip is tested on every course in the tree.

**In an AI harness.** Point the agent at [SKILL.md](SKILL.md): it explains how to find a lesson, how to run it with a
learner, and how to propose new ones. Or connect the MCP server, which gives an agent four tools over this tree:

```json
{ "mcpServers": { "openlessons": { "command": "openlessons", "args": ["mcp", "--root", "/path/to/openlessons"] } } }
```

`find_lessons`, `get_lesson` (the learner's view: answers are never returned), `validate_proposal` and `propose_lesson`.
The server is read-only unless started with `--allow-write`, and then it saves proposal files only.

### Three rules for anything that serves a learner

1. **Never send `key` to the learner.** Send `view`; keep `key` (answer, hints, feedback) on the serving side.
2. **Choose your review bar.** Each record has a `status`: `draft`, `proposed`, `reviewed` or `published`. Serve children
   `published` content only.
3. **Give credit.** Each lesson carries its `licence` and `attribution`; each asset its own. A line such as
   "Lesson from openlessons contributors, CC BY 4.0" with a link is enough.

## Quality gates

Every change must pass seventeen gates: schema, content-derived ids, references, standards terms, alignment, reading level
for the stated age, a verified answer, no answer in the prompt, a recording behind every heard prompt, licensed assets, no
personal data, a child-safety lexicon, no duplicates, review before `reviewed`, by a listed reviewer, nothing reviewed
changed silently against the base, and a fresh index. Each gate has a fixture it must fail and one it must pass, and the
validator checks that before it judges anything. The gates are a floor. They do not replace a teacher's review.

## Contribute

People and AI agents both can. A contribution is a pull request that adds files in the layout above, or one proposal file
that `openlessons apply` places for you; it says who or what made it, it must pass the gates, and a person other than the maker, listed in [REVIEWERS.json](REVIEWERS.json), reviews it before it is marked `reviewed`.
That list is empty today, which is why everything here is still `proposed`.
Start with [CONTRIBUTING.md](CONTRIBUTING.md).

Found a problem in a lesson, especially one that is unsafe for a child? See [SECURITY.md](SECURITY.md).

## Roadmap

1. Done: the format, seventeen gates, `validate`, `find`, `index`, `export`, `import`, `apply`, `observe`, `mcp`, heard
   prompts, the reviewer list, the seed.
2. Next: reviewers named, and the first `reviewed` content, with educators.
3. Then: recordings for the audio cues; more frameworks at objective level.
