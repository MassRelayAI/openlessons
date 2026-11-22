# Scoring the commons

`openlessons observe` prints what a scan saw as `observation/1` records. The rules in `rules/` (`rule/1`) say how a score
ledger reads them. Both follow the formats of [scoregraph](https://github.com/MassRelayAI/scoregraph); nothing here
depends on its code.

| rule | one observation per | passes when |
|---|---|---|
| `quality.<gate>` (15) | gate, for the whole tree | the gate has no findings |
| `feature.coverage.level.taught` | standards level and locale | at least one lesson reaches the level |
| `feature.coverage.level.reviewed` (advisory) | standards level and locale | at least one of those lessons is `published` |
| `feature.coverage.level.exact` (advisory) | standards level and locale | at least one `exact` alignment has been reviewed |

A level that nothing reaches is observed once, with locale `any`. The facts of a coverage observation: `skills`,
`lessons`, `lessons_reviewed`, `lessons_published`, `items`, `alignments`, `exact_reviewed`, `exact_proposed`, `partial`,
`related`, `objectives`, `objectives_reached`. `quality.loop.base-respected` is `unmeasured` unless `--base` is given.

```bash
V=$(git rev-parse HEAD)
openlessons observe --version $V --base origin/main --out observations.jsonl
scoregraph selftest --rules scoregraph
scoregraph ingest --db ledger.db --rules scoregraph observations.jsonl
scoregraph score  --db ledger.db --rules scoregraph --product openlessons --version $V
```

The rule files are generated with the gate list; a test checks that every observed rule has one.
