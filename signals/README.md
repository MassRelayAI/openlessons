# Usage signals

Counts that tell the gap report what is wanted, sent by products that use openlessons. **Numbers only.**

One line per count in a `*.jsonl` file, as `signal/1` ([schema](../schemas/signal.schema.json)):

```json
{"schema":"signal/1","period":"2026-W47","objective":"maths.calc.add-within-10","band":"b05-06","locale":"id","kind":"miss","count":140,"source":"example-harness"}
```

- `kind` is `request` (a harness looked for content for this cell) or `miss` (it found none it could serve).
- `period` is an ISO week or a month, never a day.
- `count` is at least 20. A product drops smaller counts before it sends anything, so no line can point at one learner.
- `source` is the public handle of the product.

The schema allows no other field: there is nowhere to put a learner, a device, a session, an age, a place or free text,
and the gates refuse a line that tries. A signal adds `5 x floor(log2(1 + count))` to a cell's demand, so the standards
stay the first voice and one busy product cannot drown them.

Nothing is here yet. Signals arrive as pull requests, like everything else.
