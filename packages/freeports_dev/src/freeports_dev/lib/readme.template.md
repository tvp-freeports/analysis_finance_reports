# {name}

![grants](validation/report/badges/grants-total.svg)
![coverage](validation/report/badges/grants-coverage.svg)
![check-grants](validation/report/badges/check-grants.svg)

![ci](ci/report/badges/ci-status.svg)
![tests.formats.integration](ci/report/badges/tests-formats-integration.svg)
![tests.formats.single_page](ci/report/badges/tests-formats-single_page.svg)
![docs.python](ci/report/badges/docs-python.svg)
![lint.python](ci/report/badges/lint-python.svg)

A **freeports formats repository**: the format definitions under `content/`, the reference outputs
their test suites are checked against under `tests/formats/`, and the signed statements about both
under `validation/`.

```sh
freeports-dev test                    # run every format's test suite
freeports-dev make-tests <FORMAT>     # write a format's reference output
freeports-validate check-grants       # verify every claim made in validation/
```

## What this repository has been vouched for

<!-- freeports-validate:begin -->
<!-- freeports-validate:end -->

### The same claims, six other ways

Each page below is one of the three lookup subcommands written down for the whole repository at
once, rather than for the one file, contributor or methodology you would name on the command line.

| Page | The same view as |
|---|---|
| [By file — who vouched for it](validation/report/file-contributor.md) | `freeports-validate who-grants <file>` |
| [By file — under which methodology](validation/report/file-methodology.md) | `freeports-validate who-grants -m <file>` |
| [By contributor — the methodologies they used](validation/report/contributor-methodology.md) | `freeports-validate granted-by <contributor>` |
| [By contributor — the files they vouched for](validation/report/contributor-file.md) | `freeports-validate granted-by -f <contributor>` |
| [By methodology — the files it covers](validation/report/methodology-file.md) | `freeports-validate granted-with <methodology>` |
| [By methodology — who adopted it](validation/report/methodology-contributor.md) | `freeports-validate granted-with -c <methodology>` |

## What a commit here has to clear

`ci.yaml` at the root says how this repository is gated, and it is the only file to open to answer
that. `freeports-dev branch-class` says which class the branch you are on is in and the rule that
put it there — run it first whenever the hook does something you did not expect.

| Branch class | What happens |
|---|---|
| `prod` — `main`, `release/*` | a missed threshold, a failing suite, or a `validation_sha256` that moved without `info.version` **refuses the commit** |
| `dev` — the default | everything is measured and reported; nothing is refused |
| `off` — `experimental`, `wip/*` | the hook does nothing at all |

The hook runs the fast tests, measures four things, refreshes both reports, and asks
`freeports-dev ci-check` for a verdict. The measurements are:

```sh
freeports-dev coverage       # how many documents are tested, and how
freeports-dev lint-score     # ruff's findings on content/, as a score out of ten
freeports-dev doc-coverage   # public objects carrying a docstring
freeports-dev ci-check       # the verdict table, and the exit status
```

`freeports-dev coverage` counts **documents**, not formats: a format with one `report.pdf` is one
document, and a format with several subdirectories is one document each. A document counts for
*integration* when it has an `out/` — that is, when a whole-document test exists at all — and for
*single page* when at least one of its pages carries all three of `<n>-pdf_blks.json`,
`<n>-txt_blks.json` and `<n>-results.json`. The two are kept apart because a repository can be
strong in one and weak in the other, and one number would hide it.

Seed a threshold in `ci.yaml` only at a figure you have measured. One set above the baseline
refuses the first commit made under it, and then it is the gate somebody switches off rather than
the code somebody fixes.

### What the last run found

<!-- freeports-dev:begin -->
<!-- freeports-dev:end -->

The same figures are written out two more ways beside it: [what each threshold is a threshold
*of*](ci/report/thresholds.md), and [where each figure comes from](ci/report/breakdown.md). There is
an [HTML page](ci/report/index.html) carrying all three as tabs, and `freeports-dev ci-report
--format json` is the model every one of them is drawn from.

### The fingerprint, and the version

`info.validation_sha256` in `package.yaml` is the hash of the hashes of **every file a grant
covers**, in `LC_ALL=C` order, with the path inside each hashed line — so moving a file counts as
much as editing it. You can check it by hand:

```sh
printf '%s\n' <the granted paths> | LC_ALL=C sort | xargs sha256sum | sha256sum
```

When it moves, `info.version` must move too. Which component is your choice: the change may be a
correction or a whole new format, and only you know which. On a `dev` branch the hook warns and
does **not** write the new fingerprint — writing it would leave the manifest claiming that version
X covers content Y, which is a false statement, and the point of the field is that it is not one.

### Keeping all of it current

The `pre-commit` hook in `.githooks/` regenerates the badges, the block above and the six pages,
and adds what it rewrote to the commit being made. It never refuses a commit over the report: a
methodology page that could not be resolved leaves the figures as they were and says so, because a
repository whose documentation server is down still has to be committable.

To do the same by hand, run what that hook runs — one walk of the repository, rendered several
times:

```sh
freeports-validate collect > model.json
freeports-validate report --model model.json --format badges --out validation/report/badges/
freeports-validate report --model model.json --format markdown --out README.md
```

`.githooks/pre-commit` has the six `--table` lines that follow.

The CI report is the same idea for the other half of what this repository publishes about itself,
and its hook block does the same thing:

```sh
freeports-dev ci-report --format json > ci-model.json
freeports-dev ci-report --model ci-model.json --format badges   --out ci/report/badges/
freeports-dev ci-report --model ci-model.json --format markdown --out README.md
```

Neither report can refuse a commit. What may refuse is `freeports-dev ci-check`, and only on a
`prod` branch.
