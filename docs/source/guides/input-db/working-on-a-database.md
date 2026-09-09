# Getting and changing a database

## Everything is validated on load

Company names unique; every bud and regex **of the main table** consistent with its own company's
name, as above; dates in `YYYY-MM-DD`; ticker symbols two to six upper-case letters; every
cross-reference — a list naming a company, a ticker naming a market, an additional bud or regex
naming a company — pointing at something that exists.

Additional buds and regexes are checked for that last cross-reference, plus — buds only — that they
are already normalised. A regex among them is not even compiled until the run needs it, so a syntax
error in one is reported when the matchers are built rather than when the file is read.

The database is read before any PDF is opened, so a mistake in it is reported in seconds rather than
after a long run produced a table with a company quietly missing from it.

## Getting an input database
There are two commands, and which one you want depends on whether the database is a *dependency* of
a formats repository or a thing you intend to maintain.

`freeports-dev setup-input-db` writes a minimal database with a single list called `TEST` under a
formats repository's `tests/input_db/`, which is what that repository's own tests use. It is a
fixture, not a starting point.

`freeports-dev init-input-db <path>` starts a **database of your own**, anywhere:

```console
$ freeports-dev init-input-db ~/work/my-db
```

It writes the two directories and all seven tables — empty, but every one of them present with its
header row. That completeness is deliberate: the engine requires all seven, so a skeleton omitting
the file you have nothing to put in yet would fail on your first run for a reason unrelated to what
you were doing. Alongside them it writes `metadata.yaml` — the manifest naming the schema version
and describing the database — and a `.gitignore`, and offers to `git init`, because which companies
matter is a curated position that deserves a history.

Pass `--sample` to have the tables filled with the packaged example database — the `TEST` list and
the companies in it — when you would rather edit an example down than write the first row from
nothing.

Either way, the shape is small enough to edit in a spreadsheet, and the validation is strict enough
to catch what a spreadsheet gets wrong.

## Working on a database
There is nothing to build and no test suite of its own. A database is seven CSV files, and the loop
is the engine's own validation:

```text
edit the CSVs  →  point a run at the database  →  read the error  →  edit again
```

**Edit.** In a spreadsheet or a text editor, whichever suits the change. Two things a spreadsheet
will try to do to you: reordering rows, which changes matching because it is first-match-wins, and
"helpfully" rewriting a date or stripping a leading zero from a ticker. Check the diff before
committing, and keep the header row.

**Point a run at it** with `-I` / `--db-directory`:

```console
$ freeports -I ~/work/my-db -F ~/work/my-formats -i report.pdf -f EURIZON-EN23
```

The order in which a run fails is the useful part. The configuration is checked first — a path that
does not exist is reported in milliseconds, before anything is loaded — and the database is read
next, before any PDF is opened. So a mistake in the database costs you seconds, not the length of a
run, and it is reported as a specific row of a specific file rather than as a company quietly
missing from the output.

**Read the error, and believe it.** Almost everything the validator rejects is one of the checks
listed above: a bud in `companies.csv` that does not occur in its own company's name, a regex that
does not match the name it sits next to, a cross-reference to a company that is not in the main
table, a malformed date or ticker. The one class of error that arrives later is a regex with a
syntax error among the *additional* patterns: those are not compiled until the matchers are built.

```{note}
There is no validate-only command today — the cheapest check is a real run, which is quick because
the database is read first. If you are curating a large database and want a faster gate, that is a
sensible thing to ask for.
```

## Which database the tests use
A formats repository's tests use the repository's **own** `tests/input_db`, the minimal one
`setup-input-db` writes, and not a curated database. This is deliberate: a format's test must fail
because the *format* broke, never because somebody added a company to a list somewhere else. Do not
point a repository's tests at a real database to make them more realistic — you would be trading a
specification for a moving target.

## Changing a database somebody else maintains
Which companies matter is a question about the work being done, not about PDF parsing, and two
people using the same formats repository will legitimately disagree about it. A database is
therefore easy to fork and cheap to maintain: if your list differs, the answer is usually your own
database rather than a pull request against someone else's.
