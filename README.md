# openlessons

**An open curriculum commons: objectives, lessons and practice items as plain files, aligned to public standards, free for any
app or AI harness to use and to add to.**

- **Content** (everything under `lessons/`, `skills/`, `topics/`, `bands/`, `alignments/`, `paths/`, `assets/`): [CC BY 4.0](LICENSE-CONTENT).
- **Code and tools** (the CLI, schemas, gates): [Apache-2.0](LICENSE).
- **Standards** are referenced by identifier and link. They belong to their publishers; see [NOTICE](NOTICE).

> Status: early (0.5). The format, the gates, a standards registry, an age-band model, a topic graph, the gap report,
> the content loop and an intake for open material (OER) are here. Lessons become `reviewed` only through the sourced AI
> jury (see "How content gets reviewed"); no educator has reviewed anything yet. A product for children reads `reviewed`
> or `published` content only.

## What is in it

| | count | where |
|---|---|---|
| standards frameworks, each with its issuer, span, licence and what may be copied | 38 (6 whose wording may be quoted, 3 by identifier and title, 29 by identifier only) | `standards/` |
| age bands, mapped to ten grade systems | 10 (0-2 to adult) | `bands/age-bands.json` |
| subjects, with strands and topics | 10 subjects, 86 topics | `topics/` |
| objectives (skills), each with its topic, age bands and prerequisites | 181, with 122 prerequisite edges | `skills/` |
| alignments of objectives to standards, none reviewed yet | 559 | `alignments/` |
| sources of open material, each with its licence class (adopt, reference only, share-alike) | 16 (9 adopt, 5 reference only, 2 share-alike) | `sources/` |
| lessons | 86: two outside a course (the first jury-reviewed lessons, in Arabic maths and English science) and the rest in 10 courses: first English, early maths, first Mandarin (taught in English), first reading in Indonesian, open maths K-2 adapted from IM K-5 Math (English, Spanish), and open first-reading stories (English, Spanish, Hindi, Chinese) | `lessons/` |
| items (questions and activities, 7 kinds) | 4,259 | `lessons/**/items.jsonl` |
| languages of instruction | 7: Arabic, English, Spanish, Hindi, Indonesian, Chinese (Simplified), Chinese (Traditional) | `locales/` |
| cells to fill (objective x age band x language) | 1,583, of which 54 have a lesson | `openlessons gaps` |

The numbers come from [`index/catalog.json`](index/catalog.json) and `openlessons gaps`, which the tool generates.

## How it is organised

```
sources/<source>.json           publishers of open material: licences, class, where an item states its own licence
intake/<source>/plan.json       adoption plans: which items of a source, mapped onto which objectives
standards/<framework>/          the registry: a framework, its licence and terms of reuse, its levels and objectives (references only)
bands/age-bands.json            the one age-band model, and what each band is called in each country's grades
topics/<subject>.json           a subject, its strands and their topics
skills/<subject>/<strand>/      one objective per file, in our own words, with its topic, age bands and prerequisites
alignments/<framework>/         objective <-> standard, with a strength: exact, partial or related
lessons/<locale>/<subject>/<shard>/<lesson id>/
    lesson.json                 what to teach, for which ages, which objectives
    items.jsonl                 one activity per line; `view` is what a learner sees, `key` is the answer
paths/<locale>/<subject>/       an ordered course of lessons
signals/                        usage counts per cell (numbers only, never anything about a learner)
runs/                           the report of every loop run
index/                          generated search index; never edited by hand
schemas/                        JSON Schema for every record
gates/                          the quality gates' data and their red and green fixtures
```

Subject, strand, topic and objective are the four levels of the topic graph. An objective's id is stable
(`maths.calc.add-within-10`); its file says which age bands it is taught in, what must come first, and which languages
it applies to. [`index/objectives.jsonl`](index/objectives.jsonl) holds every objective on one line with its standards
and the languages it already has lessons in.

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
openlessons registry            # every framework, its licence class and what may be copied
openlessons gaps --top 20       # what is missing, most wanted first
openlessons validate            # proves every gate on its fixtures, then checks the whole tree

# one course out: for a learner's device (no answers), or as an app pack (keyed strings, answer groups)
openlessons export en/maths/early-maths --format learner --min-status proposed --out out/
openlessons export en/maths/early-maths --format pack    --min-status proposed --out out/pack
```

`export` takes `published` content unless you lower the bar yourself with `--min-status`. An app pack reads back with
`openlessons import`; the round trip is tested on every course in the tree.

**In an AI harness.** Point the agent at [SKILL.md](SKILL.md): it explains how to find a lesson, how to run it with a
learner, and how to draft new ones. Or connect the MCP server, which gives an agent six tools over this tree:

```json
{ "mcpServers": { "openlessons": { "command": "openlessons", "args": ["mcp", "--root", "/path/to/openlessons"] } } }
```

`find_lessons`, `get_lesson` (the learner's view: answers are never returned), `gap_report`, `get_work_orders`,
`validate_proposal` and `propose_lesson`. The server is read-only unless started with `--allow-write`, and then it saves
proposal files only.

### Three rules for anything that serves a learner

1. **Never send `key` to the learner.** Send `view`; keep `key` (answer, hints, feedback) on the serving side.
2. **Choose your review bar.** Each record has a `status`: `draft`, `proposed`, `reviewed` or `published`. **Serve children
   `reviewed` or `published` content only: a child never receives unchecked AI output.** `export` and `get_lesson` default
   to `published`.
3. **Give credit.** Each lesson carries its `licence` and `attribution`; each asset its own. A line such as
   "Lesson from openlessons contributors, CC BY 4.0" with a link is enough. A lesson adapted from open material says
   so in its `attribution` and carries the original's credit in `adopted`: show both (see "Adopting open material").

## Where the benchmarks come from

Every framework is one folder under `standards/`, and `openlessons registry` lists them. Each records its issuer, country
or region, subjects, age and grade span, official address and version, and its **terms**: the licence, the page where the
terms were read, a short quotation from that page, the day it was read, and what follows for us.

| what the terms allow | we store | frameworks |
|---|---|---|
| reuse of the wording (CC BY, the Open Government Licence, a government reuse licence) | the publisher's wording, with its notice | National curriculum in England, the early years foundation stage, the Australian Curriculum, Scotland's Curriculum for Excellence, Spain's primary decree (BOE), Japan's Course of Study |
| reuse with a limit, or an open licence we have not yet read node by node | identifiers and official titles; statements in our own words | Common Core, Taiwan's curriculum guidelines, PISA 2025 |
| nothing (all rights reserved, NonCommercial, NoDerivatives, ShareAlike), or terms nobody could read | the code and our own paraphrase, never their text | NGSS, CSTA, CEFR, IB, Cambridge, UNESCO, ISTE, CASEL, Kurikulum Merdeka, Singapore MOE, NCERT, China's curriculum standards, HSK, YCT, and 15 more |

A gate enforces the table: a record that holds a publisher's wording under terms that do not allow it turns
`standards.terms-respected` red, and terms nobody has read allow identifiers only. A second gate,
`content.no-copied-text`, reads our own wording for the marks of copied material and for runs of words from texts we may
not copy: ten of the held-back frameworks have a fingerprint list made from their official text (hashes only, never the
text), and each of the other 22 says in its record that the paraphrase rule is its only guard (`terms.guard`).

## Adopting open material

Not everything has to be written from scratch. Openly licensed lessons, stories and textbooks already exist, and the
commons can take them in, under four rules.

1. **The licence is read per item, never per site.** A publisher's site can say "CC BY", yet one book on it is
   NonCommercial, or a newer edition is under other terms. IM's first edition is CC BY 4.0, but its v.360 is CC BY-NC 4.0.
   OpenStax moved most of its books to CC BY-NC-SA in 2026. So the importer takes the licence from the item itself (an IM
   lesson page's notice, a StoryWeaver story's licence line). An item that states no licence is refused.
2. **Only CC BY, CC0 and public domain enter the core.** These are the `adopt` licences (`gates/data/licences.json`,
   `text.adoptable`). NonCommercial, NoDerivatives and closed material is **reference only**: we keep its codes and
   links, never its content. ShareAlike material (Wikibooks, many UNESCO titles) would bind every lesson built from it,
   so it stays **out of the CC BY core**. Decided on 2026-11-23: share-alike material stays out of this repository,
   and a separate share-alike pack is made only if a product needs it.
3. **Credit travels with the item.** Every adopted lesson has an `adopted` block with these fields:
   - the source;
   - the item's own licence, and `licenceUrl`, the deed of that licence;
   - the authors;
   - the attribution text the publisher asks for;
   - `sourceUrl`, the original's address;
   - what we changed.

   The lesson's own `attribution` names the first author.
4. **Everything adopted lands as `draft`** and passes every gate like any other record. Review still decides what a
   child sees.

Two gates enforce the rules:

- **`content.source-licensed`** refuses each of these:
  - an unregistered source;
  - a source whose class contradicts its licences;
  - an item under an NC, ND, SA or unknown licence;
  - a licence the source does not publish under;
  - a deed that does not match the licence;
  - a missing attribution, author or original address;
  - an address on another host.
- **`content.no-copied-text`** accepts adopted wording only where its credit travels with it. The same run of eight
  words in a lesson without that `adopted` block is refused. The credit may carry the publisher's © and notice, but a
  credit that names a NonCommercial or NoDerivatives licence is refused.

**The sources.** `openlessons sources` lists them. Each record under `sources/` gives these:

- its class;
- the licences it publishes under;
- where an item states its own licence;
- the page where the terms were read, with a quotation and the date;
- the credit the publisher asks for;
- what could not be verified.

| class | sources (read 2026-11-23) |
|---|---|
| adopt | IM K-12 Math, first edition (CC BY 4.0; v.360 is NC, refused per item); StoryWeaver (CC BY 4.0); African Storybook (CC BY 4.0); Global Digital Library (CC BY; its BY-SA books refused); OpenStax (only the books still CC BY 4.0); Siyavula (only the unbranded CC BY 3.0 ePUBs; the ND editions refused); Project Gutenberg (public domain in the USA: check each country); Book Dash (CC BY 4.0); Wikimedia Commons (per file, as assets) |
| reference only | Khan Academy (CC BY-NC-SA), CK-12 (its own non-commercial licence), Eureka Math / EngageNY (CC BY-NC-SA 4.0), Let's Read (NC, until a title is read), Bloom Library (the licences it names are NC; its book API needs a key) |
| share-alike, kept out of this repository | Wikibooks (CC BY-SA 4.0 and GFDL), UNESCO open access (mostly CC BY-SA 3.0 IGO) |

**How to add a source.**

1. Find the publisher's own licence page, and the place where one item states its own licence.
2. Write `sources/<id>.json` (`source/1`). Fill in the class, the licences, `perItem`, the terms with their address, the
   date read and a short quotation, and the credit wording.
3. `openlessons validate` refuses a class that contradicts the licences.
4. If items will be imported, an adapter in `crates/openlessons/src/adopt.rs` reads the publisher's format into an
   intake. Its tests must show a NonCommercial item refused.

**How to import.**

1. Write an adoption plan, `intake/<source>/plan.json` (`adopt-plan/1`). Each entry gives these:
   - the input file, as fetched from the item's `url` (inputs are not committed);
   - the locale;
   - the objectives it maps onto;
   - the ages;
   - what to take from the item;
   - any practice items to generate beside it.
2. Run the import, then rebuild the index and validate:

```bash
openlessons adopt intake/illustrative-math/plan.json   # check each item's licence and credit, write drafts
openlessons index && openlessons validate
```

An item that fails a licence check is refused, with the reason. A lesson that fails any gate is taken out again.

Today's intake has three parts:

- **IM K-5 Math**: a K-2 slice of six lessons, three in English and three in Spanish, under `paths/<locale>/maths/im-k2-slice`.
- **First-reading stories from the Global Digital Library**: 29 level-1 stories (10 English, 9 Hindi, 6 Chinese, 4
  Spanish), first published by StoryWeaver, African Storybook and Book Dash, under `paths/<locale>/literacy/open-stories`.
  GDL serves each book's record and its EPUB without an account. The licence is read from both: the record's `license`
  term, and the licence the book's own credits page names. When the book names one, that is the licence recorded (the
  African Storybook titles say CC BY 3.0). If the two disagree in kind, the book is refused. Five books were refused at
  intake: four had a sentence too long for ages 5-7, and one named no author.
- **StoryWeaver direct**: the adapter is ready, but StoryWeaver serves page text only to a signed-in reader.

No Arabic or Indonesian story is in yet. GDL's Arabic books are all CC BY-NC-SA. Its Indonesian CC BY books with text
are first published by Let's Read, which stays reference only until its licence is read: a distributor's copy never
gets round its publisher's class.

**How attribution reaches the app.** Each lesson's `attribution` line and its `adopted` block go out in every export:

- the bundle;
- the learner bundle, which has no answers but keeps the credit;
- the app pack (`ext.adopted`), which `import` restores.

Bundles also carry `credits`: one entry per adopted lesson, with its attribution, licence, deed and original address.
That is what an app's credits screen shows. The minimum, next to the lesson, is its `attribution` line, which links the
licence.

## Age bands

One model for every country: `b00-02`, `b03-04`, `b05-06`, `b07-08`, `b09-10`, `b11-12`, `b13-14`, `b15-16`, `b17-18`
and `adult`. [`bands/age-bands.json`](bands/age-bands.json) says what each band is called in US grades, England's key
stages and years, Indonesia's kelas and fase, Australia, India, Singapore, China and Spain, the CEFR range a school
learner of a foreign language is usually working at, and ISCED. Ages 5 to 6, for example, are Kindergarten and Grade 1,
Year 1 and Year 2 of key stage 1, TK B and kelas 1 (Fase A), Foundation and Year 1, and Pre-A1.

## Run the loop

The loop grows the commons without lowering its quality. It has five steps, and only one of them needs a model.

```
gap report ──► pick ──► draft ──► gates ──► land as `draft` ──► a pull request with a run report
no model       no model   a model    no model   no model           a person reviews before anything is `reviewed`
```

1. **Gap report.** `openlessons gaps` lists every cell (objective x age band x language) with no lesson or too few items,
   ranked by demand. Demand comes from the standards first: one point for each framework of the registry that teaches the
   subject at those ages, plus the weight of each alignment (exact 10, partial 4, related 1). Usage counts under `signals/`
   add to it later; they are numbers per cell and week, at least 20, with no field for a learner.
2. **Pick.** `openlessons loop pick --top 20` writes work orders (`order/1`) to `.loop/orders.json`: the objective, its
   misconceptions, the ages and reading bounds, the language, the aligned standards in our own words (the publisher's
   wording only where its terms allow), and the rules. A pick is dealt across subjects and languages, prerequisites first.
3. **Draft.** Any harness answers each order with one proposal file (`proposal/1`) under `.loop/drafts/`.
4. **Gates.** `openlessons loop land` judges each proposal with all twenty-five gates. A draft that fails leaves no trace and is
   listed in the report with the reason.
5. **Land.** What passes is placed at status **`draft`**, the index is regenerated, the whole tree is judged again, and a
   run report is written to `runs/<id>.json` and `.md`: gaps closed, items added, gates passed, drafts refused. Without a
   flag the run is committed on a branch `loop/<id>`, pushed, and opened as a pull request; with `--dry-run` it stays a
   local branch; with `--no-git` it only writes files.

**Review is not part of the loop.** A draft becomes `reviewed` only through a person listed in
[REVIEWERS.json](REVIEWERS.json) for that role, language and subject. The list is empty until the project owner names
reviewers, so drafts accumulate and nothing reaches a child.

### From Claude Code, or any coding agent

```bash
openlessons loop pick --top 20 --subject maths,science,computing
# ask the agent: "Read SKILL.md section 4, then answer every order in .loop/orders.json
#                 with one proposal file under .loop/drafts/"
openlessons loop land --dry-run        # a local branch loop/<id> and a report; nothing is pushed
openlessons loop land                  # the same, pushed, as a pull request
```

### From any MCP-capable harness

Connect the server (`openlessons mcp --root <clone> --allow-write`). The agent calls `gap_report`, then
`get_work_orders`, drafts, checks each draft with `validate_proposal`, and saves it with `propose_lesson`, which writes a
proposal file under `proposals/` and nothing else. A maintainer then runs `openlessons loop land --from proposals`.

### With a drafter command

A drafter is any program that reads one order on standard input and writes one proposal on standard output: a script
around a model API, a local model, a person's editor.

```bash
openlessons loop run --top 20 --drafter "python tools/my_drafter.py" --dry-run
```

### From CI, on a schedule

[`.github/workflows/gaps.yml`](.github/workflows/gaps.yml) runs the **gap report only**, every Monday and on demand. It
uses no model and no secret; the report is the job's summary and an artifact. Drafting never runs there: it runs where a
person or an approved runner holds a model key, with one of the three ways above, and its output is a pull request that
the ordinary CI gates judge like any other.

## How content gets reviewed

A lesson or an item becomes `reviewed` in one way only: a **sourced AI jury** decides it, under the policy in
[review/jury-policy.md](review/jury-policy.md) (jury `jury-v1` in [REVIEWERS.json](REVIEWERS.json); accountable owner:
MassRelay).

- **Three roles, decided apart.** `subject` (is it right, against cited sources), `safety` (is it safe and right for the
  age), `language` (is it good language and culture for its locale). Each is filled by a separate model session that sees
  its own work order and checklist and never another role's verdict.
- **Unanimous or draft.** All three pass, or the lesson stays a `draft` with the objections written down for the next
  draft. A lesson leaves draft whole: the lesson and every item.
- **Sourced.** Every correctness claim cites a standards-registry entry aligned to the lesson's objective and a real
  reference: a web address with the date it was read, or a public-domain or CC BY source. An unsourced claim fails.
- **Audited.** Each decision is a file, `review/decisions/<record id>.json`: the policy version, the models of all three
  roles, their verdicts, checks, sources and notes, the date, and the hash of the exact content decided. Change the
  content and the decision lapses.
- **Spot-checked.** 5% of passes are drawn (reproducibly) and marked `spotCheck: pending` for an optional human check.
  It never blocks; disagreement is how jury drift is caught.

Before the jury, the gates `item.hint-fits` (no hint points at a picture the item does not show, or speaks of "both"
over three choices) and `item.objective-listed` (no item tests an objective its lesson does not list) refuse what the first
run showed a gate can catch: `jury orders` will not hand such a lesson to a reviewer. The gate `review.jury-decision` refuses a `reviewed` lesson or item without its decision, or with one that is not
unanimous, not sourced, or about other content.

```sh
openlessons jury orders --lesson <les_id> --out .jury/orders      # one order per role; hand each to a separate reviewer
# any harness writes .jury/verdicts/<role>/<les_id>.json (or uses the MCP tools jury_order / jury_submit)
openlessons jury run --harness <name> --independence "<how the roles were kept apart>" --dry-run
openlessons jury run --harness <name> --independence "<...>" && openlessons index
```

The program never calls a model: the harness that fills the roles brings its own model and key. Skills, alignments and
paths are still settled by people listed in `REVIEWERS.json`; none is listed yet.

## Quality gates

Every change must pass twenty-five gates: schema, content-derived ids, references, standards terms, alignment, a place in the
topic graph, matching age bands, reading level for the stated age, a verified answer, no answer in the prompt, a hint that fits its item, items only on the lesson's objectives, a recording
behind every heard prompt, licensed assets, adopted material under its own open licence and with its credit, no
personal data, no copied text, a child-safety lexicon, no duplicates,
review before `reviewed`, by a listed reviewer, a unanimous and sourced jury decision for every reviewed lesson and item,
nothing reviewed changed silently against the base, and a fresh index.
Each gate has a fixture it must fail and one it must pass, and the validator checks that before it judges anything. The
gates are a floor. They do not replace a teacher's review.

## Contribute

People and AI agents both can. A contribution is a pull request that adds files in the layout above, or one proposal file
that `openlessons apply` or `openlessons loop land` places for you; it says who or what made it, it must pass the gates,
and the jury (above) decides it before it is marked `reviewed`. Start with [CONTRIBUTING.md](CONTRIBUTING.md).

Found a problem in a lesson, especially one that is unsafe for a child? See [SECURITY.md](SECURITY.md).

## Roadmap

1. Done: the format, twenty-five gates, the sourced AI jury, the registry, the age bands, the topic graph, the gap
   report, the loop, `export`, `import`, `mcp`, heard prompts, the reviewer list, the seed, the open-material intake
   (`sources/`, `openlessons adopt`).
2. Next: human spot-checks of jury passes, and educators named for skills and alignments; Arabic and Indonesian stories
   from a source whose licence is read.
3. Then: usage signals from products, with no learner data; recordings for the audio cues; objective-level nodes for the
   open frameworks.
