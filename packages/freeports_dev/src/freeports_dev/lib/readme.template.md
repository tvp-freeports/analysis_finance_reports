# {name}

![grants](validation/report/badges/grants-total.svg)
![coverage](validation/report/badges/grants-coverage.svg)
![check-grants](validation/report/badges/check-grants.svg)

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
