# `freeports-dev`

## The `freeports-dev` subcommands
| Subcommand | Does |
|---|---|
| `init-format-repo <path>` | writes an empty formats repository at `<path>` |
| `setup-input-db` | writes `tests/input_db/` with a minimal `TEST` company list |
| `init-input-db <path>` | writes an empty input database at `<path>`, to maintain as its own repository |
| `inspect-document` | classifies the pages of a report: which page is what |
| `inspect-page` | shows what one page looks like at a chosen stage of the pipeline |
| `make-tests` | freezes one page's current behaviour as JSON fixtures |
| `test` | runs the repository's tests through pytest |

They are meant to be used in that order the first time and in the
`inspect-document → inspect-page → make-tests → test` loop after that; {doc}`../../guides/formats/writing-a-format`
walks through the loop with a real format, and this page is the reference for the options.

## Creating an empty repository
```console
$ freeports-dev init-format-repo ~/work/my-formats
$ cd ~/work/my-formats
$ freeports-dev setup-input-db
```

`init-format-repo` writes `package.yaml`, the `metadata/` tables, the `content/` tree and an empty
`tests/`, then validates the generated `package.yaml` against its JSON schema and says so. It is a
skeleton, not a working repository: it supports no format until you add one. See
{doc}`../../guides/formats/repository` for what each generated file is.

(what-init-format-repo-writes)=
### The README, the report and the hook

It also writes everything a repository needs to *show* what it has been vouched for, because the
alternative is that each of those files gets invented separately by every person who ever makes a
formats repository:

| Path | What |
|---|---|
| `README.md` | the repository's front page: the badges, both summary tables between their markers, and links to the pages below |
| `validation/report/<table>.md` | six pages, one per arrangement of the grants — named after the `--table` value that fills each one |
| `validation/report/badges/` | `grants-total`, `grants-coverage`, `check-grants`, each as an SVG and as shields.io endpoint JSON |
| `ci/report/<table>.md`, `ci/report/badges/`, `ci/report/index.html` | the same for what the last run measured |
| `ci.yaml` | how the repository is gated: the branch classes, the thresholds, the key server |
| `Makefile`, `make.bat` | the single entry point — every target, and the Windows shim that calls it |
| `.gitignore` | what a run leaves behind: `reports/`, the caches, a stray log |
| `.githooks/pre-commit` | names the Makefile target that is the commit gate |

All nine are then **filled straight away**, by running the command once — so a repository never
starts life with three broken images and six empty tables in it. That step is best-effort: if
`freeports-validate` is not installed, or the methodology pages cannot be resolved from this
machine, the initialisation says so, names the command to run later, and succeeds anyway. A skeleton
that could not be created without a reachable documentation server would be a worse tool than one
that leaves nine files to fill.

**The hook runs one thing**: `make pre-commit` on a `dev` branch, `make ci-full` on a `prod` one.
What the gate consists of is decided in the `Makefile`, so it can grow without the hook being
edited — and every step of it is a target you can also run by hand, which is the property that makes
a refused commit something you can reproduce rather than something that happened to you. What is
left in the hook is the environment check, the branch class, the non-ASCII filename check, the
staging of what the run rewrote, and the verdict.

**No report can refuse a commit**, on any branch. The gate collects once, renders the artefacts, and
the hook `git add`s the ones it actually rewrote, so the refresh is part of the same commit as the
change that caused it. Everything about that is conditional: no `freeports-validate` on the path, no
methodology it can resolve, or a page whose marker pair somebody removed, and the target says so on
standard error and leaves the file alone. The figures depend on a network, and a committer on a
train still has to be able to commit.

To change what the gate does, edit the `Makefile` — `make help` lists every target, and
{doc}`../../guides/devops/the-gate` explains which of them may refuse and on which branch.

`setup-input-db` copies a minimal input database into `tests/input_db/`, with a single list named
`TEST`. It exists so that the tests of a repository do not depend on a database maintained
elsewhere — a format's test must fail because the *format* broke, never because someone edited a
company list. For real runs you want a real database; see {doc}`../../guides/input-db/index`.

`init-input-db` is the other half of that sentence and is not part of this loop: it starts a
database you intend to maintain, at a path of your choosing, as its own repository rather than
inside a formats repository. `--sample` fills its tables with the same example data
`setup-input-db` copies, as something to edit down. See {doc}`../../guides/input-db/index`.

## `inspect-document` — which page is what

```console
$ freeports-dev inspect-document --format CARNE-EN23
Page 1: unclassified
Page 2: unclassified
Page 25: investments
```

| Option | Meaning |
|---|---|
| `--format` / `-f` | the format to load. **Required** |
| `--page` / `-p` | report only this page (1-based); omitted, every page is reported |
| `--report` | the PDF. Defaults to `tests/formats/<FORMAT>/report.pdf` |
| `--repo` / `-r`, `--config`, `--db-directory` / `-I` | the shared options; see {doc}`../configuration/dev-and-validate` |

Run this before anything else. A page that should be `investments` and comes back `unclassified` is
a classification problem, and nothing downstream of it can be right until it is fixed — chasing the
extraction of a page the engine never selected is the single most common way to lose an afternoon.

`--page` narrows what is **printed**, not what is classified: the whole document is classified either
way. A format may supply a finalizer that rewrites the raw per-page answers looking at all of them
together — *every page after the holdings header is holdings* is the usual shape — so a page
classified on its own can get a different answer from the same page classified in its document, and
the isolated one is the wrong one.

## `inspect-page` — what the engine sees, stage by stage

```console
$ freeports-dev inspect-page --format CARNE-EN23 --page 25 --page-type investments
```

| Option | Meaning |
|---|---|
| `--format` / `-f` | the format. **Required** |
| `--page` / `-p` | the page number, 1-based. **Required** |
| `--page-type` / `-t` | the page class to process it as. Default `investments`, or `dev.page_type` |
| `--mode` / `-m` | what to print — see below. Default `results` |
| `--strings` | the strings to look for, **required** by the three line-set modes |
| `--report` | the PDF. Defaults to `tests/formats/<FORMAT>/report.pdf` |
| `--filter-data` | a `.pkl` of filter data. Defaults to the repository's test companies |
| `--target-list` / `-T` | which lists those test companies come from. Default `TEST` |
| `--repo` / `-r`, `--config`, `--db-directory` / `-I` | the shared options; see {doc}`../configuration/dev-and-validate` |

The modes fall into three families, and choosing the right family is most of the skill:

| `--mode` | Prints | Use it to |
|---|---|---|
| `pdf_blks` | the blocks after `pdf_extract` | see whether the *layout* predicate found anything |
| `txt_blks` | the blocks after `text_filter` | see whether the *meaning* was read out of them |
| `results` | the deserialized rows | see the finished product of the page |
| `table_md`, `table_ascii` | the PDF blocks rendered as a table | read a tabular page as a table, with its row and column metadata |
| `structured`, `semistructured`, `unstructured` | which lines match `--strings`, as that level's selection sees them | learn what font, size and band a value is actually set in |

The first three are the diagnosis: running them in order tells you *which of the three segments* is
wrong, which is a much smaller question than "the page is wrong". The line-set modes answer the
question that comes before writing a selection at all:

```console
$ freeports-dev inspect-page -f CARNE-EN23 -p 25 -m structured --strings "Total Assets"
```

`--page-type` matters even in the pipeline modes: it is what decides which pipeline the page is fed
through, so inspecting a page as the wrong class shows you a correct answer to the wrong question.

## `make-tests` — freeze a page

```console
$ freeports-dev make-tests --format CARNE-EN23 --page 25 --page-type investments
```

| Option | Meaning |
|---|---|
| `--format` / `-f`, `--page` / `-p` | as above. Both **required** |
| `--page-type` / `-t` | as above. Optional: `dev.page_type` can supply it, and it defaults to `investments` |
| `--document` / `-d` | the document variant, for a format with several |
| `--report`, `--filter-data`, `--target-list`, `--repo`, `--config`, `--db-directory` | as above |
| `--noconfirm` | do not ask before writing each fixture. Also `dev.noconfirm` |
| `--skip-pdf-blks`, `--skip-txt-blks`, `--skip-results` | leave that fixture out |
| `--print_pdf_blks`, `--print_txt_blks` | also print those blocks while generating |
| `--noprint_results` | do not print the results while generating |

It writes the three per-page fixtures — the PDF blocks, the text blocks, the results — as JSON under
`tests/formats/<FORMAT>/pages/`, after showing you what it is about to record and asking. Read what
it prints before answering: `make-tests` records *what the code does now*, so confirming a wrong
result promotes a bug into the specification, and the test will then defend it.

`--noconfirm` is for regenerating fixtures you have already reviewed, not for generating new ones.
As with the engine's boolean flags, the flag can only switch it **on**; to make it a standing choice
set `dev.noconfirm` in the configuration file, and to switch that back off write `false` there —
an absent flag says nothing, it does not say no.

## `test` — run the repository's tests

```console
$ freeports-dev test                         # the per-page tests — the default
$ freeports-dev test --all                   # both halves
$ freeports-dev test --slow                  # only the whole-document tests
$ freeports-dev test --format CARNE-EN23     # one format
$ freeports-dev test --repo ~/their-formats  # a repository that is not yours
$ freeports-dev test -- -x -k investments    # anything after `--` goes to pytest
```

| Option | Meaning |
|---|---|
| `--fast` | only the per-page tests, recorded as the suite `formats.single_page`. **The default** |
| `--slow` | only the whole-document tests, recorded as `formats.integration` |
| `--all` | both |
| `--format` / `-f` | narrow the run to one format |
| `--repo` / `-r`, `--config`, `--db-directory` / `-I` | the shared options; see {doc}`../configuration/dev-and-validate` |

**The three selections are the Makefile's three targets**: `--fast` is `make test-fast`, `--slow` is
`make test-slow`, `--all` is `make test-all`, and the default matches `make test`. The two surfaces
are one vocabulary on purpose — `make` is how you work on your own repository, this command is how
you interrogate one that is not yours, and a person meets both. A `-m` of your own, after `--`,
outranks all three and is left to stand alone: pytest takes the last `-m` on the line, so a second
one added beside yours would silently replace the selection you asked for.

A run that leaves half the suite out **says so in its last line**. And a selection that matched
nothing is not a failure: a repository with no formats in it yet, or one whose formats have no
whole-document test, has nothing there to fail.

It is pytest, with the `freeports_dev` plugin doing the collection: a directory named like a format
in `metadata/formats.csv` becomes a test node, and the per-page fixtures and the `out/` reference
become the tests under it. `--rootdir` is set to the repository, so the exit status is pytest's own
and a CI job needs nothing else.

Two kinds of test live there and they cost very different amounts. The per-page tests are fast, and
they are what you run every few minutes while working. The whole-document test replays the entire
report and compares the output tables against `tests/formats/<FORMAT>/out/`; it is slow, it is
marked `integration_tests`, and it is the one that actually says the format works. That marker is
what `--fast` and `--slow` select on, and the flag is the one spelling of it — the marker string
itself never needs to be written out by hand.

```{important}
`tests/formats/<FORMAT>/out/**` is the repository's specification, not a snapshot. When a run
diverges from it, the assumption is that the engine changed, not that the expectation was wrong.
Regenerating one of those files is a deliberate act with a reason written down — never a way to turn
a red test green — and it is also what the grants of the next section are *about*.
```
