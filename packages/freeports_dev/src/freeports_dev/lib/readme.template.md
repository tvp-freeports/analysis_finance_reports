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
make init                             # first time: point git at .githooks/
make test                             # the per-page suite — the development loop
make ci-fast                          # what the commit hook runs: lint, tests, report, verdict
make help                             # every target, with a line each
```

**Two command surfaces, one vocabulary.** `make` changes *this* repository — it writes the
measurements, refreshes the committed reports, and rewrites the manifest when it is entitled to.
`freeports-dev` and `freeports-validate` answer questions *about* a repository, including one that
is not yours, and write only where you point them:

```sh
freeports-dev make-tests <FORMAT>              # write a format's reference output
freeports-dev inspect-page -f <FORMAT> -p <N>  # look at a page while writing a format
freeports-dev test --repo <path> --all         # somebody else's repository, both suites
freeports-dev ci-report --repo <path> -f json  # their figures, as a model to query
freeports-validate check-grants                # verify every claim made in validation/
```

Every target is one of those commands with `--repo` pointed here, so the two never disagree:
`make test-fast` is `freeports-dev test --fast`, `make ci-check` is `freeports-dev ci-check`, and
`make check-grants` is `freeports-validate check-grants`. On Windows the targets are typed
`make.bat <target>` from `cmd` or PowerShell — a shim that starts a POSIX shell and calls the same
`Makefile`.

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

The hook runs one thing — `make pre-commit` on a `dev` branch, `make ci-full` on a `prod` one — so
what the gate consists of is decided in the `Makefile` and can grow without the hook being touched.
On a dev branch that is the linter, the per-page suite, three measurements, both report renderings
and the verdict; on a prod branch it is both suites and the grants re-resolved over the network as
well. Each is a target you can run on its own:

```sh
make test-fast        # the per-page suite, recorded as formats.single_page
make test-slow        # the whole-document suite, recorded as formats.integration
make coverage         # how many documents are tested, and how
make lint             # ruff's findings on content/, and the score out of ten
make doc-coverage     # public objects carrying a docstring
make ci-check         # the verdict table, and the exit status
```

**Only `make ci-check` and `make fingerprint` can refuse a commit.** Everything else measures and
records; the verdict reads what they wrote, together with `ci.yaml` and the class of the branch,
and is the one step that knows whether this branch enforces or reports.

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

The commit hook refreshes both reports and adds what it rewrote to the commit being made, so the
published figures and the change that moved them arrive together. Two targets do the same by hand:

```sh
make ci-report          # the badges, the block above, the two pages and the HTML
make validation-report  # the grant badges, its block, and the six lookup pages
make validation         # that, plus check-grants and check-keys — the whole grant picture
```

Each is **one walk of the repository rendered many times**, never one walk per artefact: the grant
report resolves every methodology page from the configured sources, and doing that once per page
would fetch each of them nine times over for an answer that cannot have changed in between.

Neither report can refuse a commit, on any branch. A methodology page that could not be resolved
leaves the figures as they were and says so — a repository whose documentation server is down still
has to be committable, and a report rewritten to say "coverage --, check-grants failing" would be a
claim about somebody's web server written into tracked files. What may refuse is a *threshold*, and
that is `make ci-check`'s decision on a `prod` branch.
