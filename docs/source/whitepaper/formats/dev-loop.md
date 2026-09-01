# The development loop

Four commands, in this order, over and over. {doc}`tooling` is the option-by-option reference; this
page is what the loop is *for*.

```text
inspect-document  →  inspect-page  →  make-tests  →  test
   which pages?      what does the      freeze it     does it
                     engine see?                      still hold?
```

## 1. `inspect-document` — which page is what

```console
$ freeports-dev inspect-document --format CARNE-EN23
Page 24: unclassified
Page 25: investments
Page 26: investments
```

**Run this first, and do not go past it until it is right.** If classification is wrong, nothing
downstream can be: a page that should be `investments` and comes back as nothing is a classification
bug, not an extraction bug, and chasing it in `pdf_extract` is the single most common way to lose an
afternoon.

## 2. `inspect-page` — what the engine sees, stage by stage

```console
$ freeports-dev inspect-page -f CARNE-EN23 -p 25 -t investments -m pdf_blks
$ freeports-dev inspect-page -f CARNE-EN23 -p 25 -t investments -m txt_blks
$ freeports-dev inspect-page -f CARNE-EN23 -p 25 -t investments -m results
```

The three pipeline modes in order, and the order is the method: each shows what one segment made of
the page, so the first one that looks wrong is the segment to fix. This turns *"the page is wrong"*
— a question with no handle on it — into *"`text_filter` dropped these blocks"*, which has one.

| First wrong at | Look at |
|---|---|
| `pdf_blks` | the layout selections: font, size, area ({doc}`selections`) |
| `txt_blks` | the filter: are the target companies matching? is the regex right? |
| `results` | the deserializer: types, columns, units |

Two more families of mode:

- `table_md` / `table_ascii` render the PDF blocks **as a table**, with their row and column
  metadata — which is how you check that the tabularizer read the columns the way you read them;
- `structured` / `semistructured` / `unstructured` take `--strings` and report which lines match,
  which is how you find out what font and size a value is set in before writing a selection at all.

## 3. `make-tests` — freeze a page

```console
$ freeports-dev make-tests -f CARNE-EN23 -p 25 -t investments
```

Writes the three per-page fixtures — PDF blocks, text blocks, results — as **JSON** under
`tests/formats/<FORMAT>/pages/`. JSON and readable on purpose: a regression should be visible in a
diff ({doc}`../design/blocks`).

```{important}
`make-tests` records **what the code does now**. It shows you what it is about to write and asks;
read it. Confirming a wrong result promotes a bug into the specification, and from then on the test
defends the bug.
```

Freeze a handful of representative pages, not every page: the first page of a table, a continuation
page, a page with an awkward row. Those are where formats break.

## 4. `test` — run them

```console
$ freeports-dev test --format CARNE-EN23
$ freeports-dev test                        # the whole repository
```

Two kinds of test with very different costs:

**Per-page tests** replay one page through one segment against its fixture. Fast, run constantly,
and they tell you *which segment of which page* changed.

**The whole-document test** replays the entire report and compares the output tables against
`tests/formats/<FORMAT>/out/`. Slow, marked `integration_tests`, and it is the one that actually
says the format works — the per-page fixtures can all pass while the schedule, the promise
resolution or the deduplication are wrong.

```{important}
`out/**` is the repository's **specification**, not a snapshot. If a run diverges from it, the
assumption is that the engine changed, not that the expectation was wrong. Regenerating one of those
files is a deliberate act with a reason written down — never a way to make a red test green. It is
also what the grants of {doc}`../validation` are *about*: a grant is a claim about the content of
exactly those bytes.
```
