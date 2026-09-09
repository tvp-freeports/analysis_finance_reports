# The validation coverage report

`freeports-validate report` walks a repository's grants once and renders what it found in several shapes at a time. This page is that command; what the resulting figures are worth is {doc}`../../guides/grants/index`, and this site's own rendering of them is {doc}`../../validation/report/index`.

## What the command does

Everything above answers a question about one thing — this file, this person, this methodology. The
report answers the question about the repository: **what has been vouched for here, by whom, under
what, and does it still hold?**

It is two subcommands, and the split is the design:

```console
$ freeports-validate collect                                    # the model, as JSON
$ freeports-validate report --format html     --out coverage.html
$ freeports-validate report --format badges   --out docs/badges/
$ freeports-validate report --format markdown --out README.md
$ freeports-validate report --format rst      --out README.rst
$ freeports-validate report                                     # the model again, the default
```

`collect` walks `validation/*.yaml` once and establishes the facts; every rendering is a **pure
function of that JSON**. So a question none of the renderings anticipated is a `jq` expression away
rather than a feature request:

```console
$ freeports-validate collect | jq '.grants[] | select(.state != "current")'
$ freeports-validate collect | jq '.methodologies[] | {name, coverage}'
```

Neither needs a key. A report is most often wanted by somebody who did not write the repository.

### What the model holds

| Key | |
|---|---|
| `sources`, `general_methodology` | what this run resolved from, and the text every document's `version` is a claim about |
| `contributors` | one per document: identity, `schema`, `signature`, `version`, how many grants, and whether it is `counted` |
| `methodologies` | name, resolution state, resolved URI and hash, the patterns it declares, one `adoptions` entry per document that adopted it, and its coverage |
| `grants` | one per granted file: path, recorded hash, `state`, and `in_scope` |
| `coverage`, `totals`, `status` | the repository-wide figures, and one word for the run |

Three of those fields are three-valued rather than two, for the same reason each time — a question
that could not be *asked* must not be answered:

| Field | |
|---|---|
| `grants[].state` | `current`, `changed`, `missing` |
| `grants[].in_scope` | `true`, `false`, or `null` when the methodology page could not be resolved |
| `status` | `passing`, `failing`, or `inconclusive` — something could not be reached, and nothing was disproved |

### How the percentage is arrived at

**A methodology that declares no paths contributes no denominator.** There is nothing it could be
complete against, and inventing one would put a meaningless percentage on a badge. For one that does
declare paths, the denominator is what its patterns match in the working tree — the files it *could*
cover — and the numerator is how many of those somebody has granted under it.

**A document that is not intact counts toward nothing.** Coverage is a statement about assurance, and
a document whose signature is missing or wrong, or whose `version` is stale, asserts nothing. Its
grants are still listed, with the state of the document they came from: an uncounted grant that were
also invisible would be a report quietly disagreeing with the repository it describes.

**The repository-wide figure is a union, not a sum.** Adding the per-methodology fractions together
would count a file twice as soon as two methodologies declared it, and a badge reading 150% is worse
than no badge.

### The renderings

| `--format` | `--out` | |
|---|---|---|
| `json` | optional | the model. The default |
| `html` | a file | one self-contained page — inline style and script, no fetch of any kind — with the three viewpoints as tabs, matching `who-grants` / `granted-by` / `granted-with`, a filter box, and every methodology name linking to the text it resolved from |
| `badges` | a **directory** | `grants-total`, `grants-coverage` and `check-grants`, each as an SVG drawn locally in the shields style and as the shields.io *endpoint* JSON for anyone who would rather fetch |
| `markdown`, `rst` | a file | one of seven tables, rewritten in place between two markers |

The Markdown and reStructuredText renderings write **into** a file somebody else wrote, between
markers you paste where the table should go:

```markdown
<!-- freeports-validate:begin -->
<!-- freeports-validate:end -->
```

```rst
.. freeports-validate:begin
.. freeports-validate:end
```

Rewriting is idempotent: running it twice changes nothing. A file with no marker pair is an **error**
naming the markers, never an append — where a coverage table belongs in a README is a judgement
about that README, and a command that guessed would put one under a licence notice and then keep it
there for ever.

Nothing the report produces carries a timestamp. A date would make every regeneration a diff saying
nothing had changed, which is how a generated block stops being regenerated. What a reader wants to
know is what the repository says now, which running the command again tells them.

### The seven tables

The HTML page shows three viewpoints as tabs. A Markdown or reStructuredText file cannot have tabs,
so it holds **one** table, chosen with `--table`.

There are seven. The repository has three dimensions — file, contributor, methodology — and a lookup
is a choice of two of them: group by the first, list the second. Taken in every order that is six,
which is exactly what the three lookup subcommands are with and without their flag. The seventh is
the summary, and it is the default, so every invocation written before `--table` existed still means
what it meant.

| `--table` | The same view as |
|---|---|
| `summary` *(default)* | one line per methodology: its grants, its coverage, who adopted it |
| `file-contributor` | `who-grants <file>` |
| `file-methodology` | `who-grants -m <file>` |
| `contributor-methodology` | `granted-by <contributor>` |
| `contributor-file` | `granted-by -f <contributor>` |
| `methodology-file` | `granted-with <methodology>` |
| `methodology-contributor` | `granted-with -c <methodology>` |

The name reads as *grouped by the first, listing the second*, and each table says in its own heading
which invocation it is the written-down form of — so a reader who has learned the lookups does not
have to learn a second vocabulary for the same repository.

Each is three columns, one row per distinct pair, with the group key repeated down the first column
rather than blanked. Joining the other dimension into one cell is what the lookup subcommands print
on a terminal, and it stops being readable the moment a methodology covers several hundred files. The
third column is the state of the grants that pair stands for — almost always one word, and two when
two documents disagree about the same file.

The `(!)` and `(?)` marks are the ones `granted-by` and its two siblings already print, and they go
on the **listed** item, never on the group key: what is out of scope is the *pair*, so marking a
heading would split one methodology into two the moment one of its grants left scope. A legend line
appears under the table only when a mark actually appears on it.

### Rendering several things out of one walk

A repository that keeps a README, six tables and three badges current has nine artefacts to write,
and collecting nine times would resolve every methodology page nine times for an answer that cannot
have changed in between. `--model` renders a model gathered earlier instead of walking again:

```console
$ freeports-validate collect > model.json
$ freeports-validate report --model model.json --format badges   --out validation/report/badges/
$ freeports-validate report --model model.json --format markdown --out README.md
$ freeports-validate report --model model.json --format markdown \
      --table methodology-file --out validation/report/methodology-file.md
```

`--model -` reads the model from a pipe, so `collect | report --model -` works as well. A `--model`
naming a file that is not there is an **error**, deliberately not a reason to walk the repository:
a run that quietly answered a different question from the one asked is the failure this whole
command exists to prevent.

### What a repository gets for free

`freeports-dev init-format-repo` creates all nine of those, already filled, plus the hook that keeps
them current — see {ref}`what-init-format-repo-writes`. So the usual answer to "how do I set this
up" is that it is already set up.
