# Gates

- `rules.json`: the list of gates. The program refuses to run when this list and the gates it implements differ.
- `data/`: what the gates read. `reading-levels.json` (bounds per age), `licences.json` (licences an asset may carry),
  `safety/<language>.json` (the safety lexicon; `hashed` entries are sha256 of lower-cased words we do not print),
  `protected.json` (marks of copied material, and fingerprints of texts we may not copy: hashes of every run of six
  words, made with `openlessons fingerprint <file>` from a text file that is never committed).
- `fixtures/green/`: a small, complete tree that passes every gate.
- `fixtures/red/<rule>/`: files laid over the green tree that the named gate must refuse.

`openlessons selftest` copies the green tree, lays each red overlay over it in turn, and requires the named gate to fire.
The green tree is also the base each overlay is compared with, which is how `loop.base-respected` is proven.
`openlessons validate` runs the self-test first and judges nothing if it fails.

## Adding or changing a gate

1. Add the rule to `rules.json` and a red overlay under `fixtures/red/<rule>/`. `openlessons selftest` now fails: good.
2. Write the check in `crates/openlessons/src/` and add the rule id to `RULES`.
3. Run `openlessons selftest` and `cargo test`. The green tree must still pass every gate.

Fixture items carry content-derived ids like any other item: after editing one, run `openlessons stamp` on **that file**.
Do not stamp `fixtures/red/layout.content-addressed/`: its wrong id is the point of the fixture. After changing the green
tree, regenerate its index with `openlessons index --root gates/fixtures/green --data gates/data`.

To add a word to a lexicon without printing it: `printf '%s' 'word' | shasum -a 256`, and put the digest under `hashed`.
