# Running the engine

Everything needed to install `freeports`, point it at a document, and get tables out. This area is
reference: it says what each thing does, not why it was built that way. The *why* is {doc}`the design
section <../design/index>`.

## The pages

| Page | Answers |
|---|---|
| {doc}`installation` | what do I install, and how |
| {doc}`inputs` | what must exist before a first run can work at all |
| {doc}`quickstart` | one real run, commented argument by argument |
| {doc}`command` | every option of `freeports`, with its defaults and its validation |
| {doc}`documents` | how to name a document: the `<url>:<path>:<name>` grammar, and `save_pdf` |
| {doc}`output` | the profiles, the tables produced, the rules that hold across all of them |
| {doc}`logging` | the three destinations, where each lands, how to read them |
| {doc}`batch` | one job per CSV row |
| {doc}`parallelism` | the two levels, `auto`, the measurements, the price in memory |
| {doc}`configuration/index` | the four sources a setting can come from, and how they merge |

## The shape of a run

One invocation of `freeports` is one or more **jobs**. A job is one report, read with one format,
against one set of target companies. Whether the reports come from `--input` given several times or
from the rows of a batch CSV, the results of every job in the run are accumulated together and
written **once**, at the end, as one set of tables.

That is worth stating plainly because it explains an otherwise surprising property: running two
reports in one invocation is not the same as running them twice and concatenating. In one
invocation they share the promise resolution and the deduplication of funds; in two they do not.

## The minimum

Four things must be given, and only one of them can ever be worked out by the engine on its own:

| What | How | Can be inferred? |
|---|---|---|
| a document | `--input` / `-i` | no |
| a format for it | `--format` / `-f` | **yes**, from a URL the formats repository recognises |
| a formats repository | `--repo` / `-F` | no |
| an input database, and at least one list in it | `--db-directory` / `-I`, `--target-list` / `-T` | no |

```console
$ freeports --input report.pdf --format EURIZON-EN23 \
            --repo /path/to/formats-repo --db-directory /path/to/input-db \
            --target-list controversial_weapons --out ./results
```

None of the four has a useful default, and the engine refuses rather than guessing. If no target
list is given the run stops immediately, before opening anything — the check is first because
failing fast on a missing input is kinder than failing after four minutes of PDF.

```{toctree}
:maxdepth: 2
:hidden:

installation
inputs
quickstart
command
documents
output
logging
batch
parallelism
configuration/index
```
