# The two inputs

The engine ships with no knowledge of any report and no idea which companies you care about. Both
are inputs, both live outside the engine, and it will refuse to guess at either. That refusal is the
design, not an omission: see {doc}`../design/formats-as-plugins`.

| Input | Says | Point at it with | Detail |
|---|---|---|---|
| **formats repository** | how to read the layouts you care about | `--formats-directory` / `--repo` / `-F` / `-r`, or `FREEPORTS_FORMATS_REPO_PATH` | {doc}`../formats/repository` |
| **input database** | which companies to look for, and how to recognise each | `--db-directory` / `-I`, plus `--target-list` / `-T` | {doc}`../input-db` |

## The formats repository

A directory, tracked in its own version control, holding one recipe — a **format** — per report
layout. `analysis_finance_reports_formats` is one such repository; there is nothing privileged about
it, and you can maintain your own.

To start an empty one:

```console
$ freeports-dev init-format-repo ~/work/my-formats
```

That gives you a skeleton that supports no format yet. {doc}`../formats/index` is how you add one.

The repository is also what makes `--format` optional: it carries a mapping from URL prefixes to
format names, so a document downloaded from a known address is read without being told which format
it is. There is no such inference for a local file — a path carries no evidence of who published it.

## The input database

Which companies a run is looking for, organised into named **lists**. `--target-list` names the
lists to use, and a run with no list stops before opening anything.

For testing, a minimal database with a single list called `TEST`:

```console
$ freeports-dev setup-input-db
```

It writes into `tests/input_db/` of the formats repository, deliberately: a format's tests must fail
because the *format* broke, never because someone edited a company list maintained elsewhere. For
real runs you want a real database.

Two properties of the database change what a run finds, and both surprise people:

- **matching is first-match-wins, in file order.** The order of the file is meaningful, not
  incidental. A run that seems to attribute a holding to the wrong company is often reporting a
  database problem rather than an extraction one;
- a company absent from every named list is simply not looked for. Nothing reports it, because
  nothing was asked about it.

{doc}`../input-db` is the full description.
