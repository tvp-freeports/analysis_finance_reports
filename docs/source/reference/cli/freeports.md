# The `freeports` command

Every option, what it sets, and what happens when it is left out. The same settings are reachable
from three other sources — see {doc}`../configuration/index` — and this page is the command-line view of
them.

## Every option at a glance
```text
freeports  -i <document>…  -f <format>  -F <formats-repo>  -I <input-db>  -T <list>…
           [-o <path>] [-P <profile>] [-z] [--separate-out] [--no-download]
           [-b <batch.csv>] [--config <file>] [-j <n>|--jobs <n>|--pages <n>] [-v…|-q…]
```

| Option | Sets | Absent means |
|---|---|---|
| `--input`, `--report`, `-i` | the documents | no document from this source |
| `--format`, `-f` | the format | infer it from a URL, or fail |
| `--formats-directory`, `--repo`, `-F`, `-r` | the formats repository | unset — the run fails |
| `--db-directory`, `-I` | the input database | unset |
| `--target-list`, `-T` | the target lists | unset — **the run fails first of all** |
| `--out`, `-o` | the output path | the working directory |
| `--out-profile`, `-P` | the output profile | `regular` |
| `--archive`, `-z` | write a compressed sibling | *nothing said* — a lower tier may still set it |
| `--separate-out` | one file per report and format | *nothing said* — likewise |
| `--no-download` | do not **keep** a downloaded PDF | *nothing said* — the default is to keep |
| `--batch`, `-b` | the batch CSV | not in batch mode |
| `--config` | use this configuration file | search the three tiers |
| `--workers`, `-j` | both parallelism levels | `auto` |
| `--jobs` | the job level alone | inherit `--workers` |
| `--pages` | the page level alone | inherit `--workers` |
| `-v`, `-q` | verbosity, counted | warnings only |

## Documents and format

### `--input` / `--report` / `-i`

Repeatable, and takes several values after one occurrence. Each value is a **document specifier** in
the `<url>:<path>:<name>` grammar — {doc}`../../guides/user/documents` is that grammar in full, including how a `:`
inside a value is quoted and what `save_pdf` does in each case.

```console
$ freeports -i a.pdf b.pdf                       # two documents
$ freeports -i "https://x/r.pdf:local.pdf:2023"  # download, save there, call it 2023
```

Giving none from this source leaves the field **unset** rather than empty, so a configuration file
or the environment can still supply the documents.

### `--format` / `-f`

The format name, as `metadata/formats.csv` of the repository synthesises it — `EURIZON-EN23`,
`FINECO-EN23@LUX`, `MEDIOLANUM-IT24.A`.

Omit it and the engine tries to infer it from the URL of each document, using the repository's
`url_mapping.csv`; the first matching prefix wins. Inference works only for documents that *have* a
URL. Two documents inferring **different** formats is an error rather than a choice made silently.
Giving `-f` explicitly while inference also succeeds and disagrees is a warning, not an error: you
were explicit, so you win, but you are told.

## Where things live

### `--formats-directory` / `--repo` / `-F` / `-r`

The formats repository. Note that the two tooling commands read a **differently named** variable for
the same thing — `FREEPORTS_FORMATS_REPO`, not `FREEPORTS_FORMATS_REPO_PATH`; see
{doc}`index`.

### `--db-directory` / `-I`

The input database.

### `--target-list` / `-T`

Repeatable, and takes several values after one occurrence. **This is the check that runs first**: a
configuration with no target list from any source is rejected before a single PDF is opened.

From the environment the value is taken **whole** and never split, so a list whose name contains a
comma still works.

## Output

### `--out` / `-o`

Where results go. A directory for `regular` and `structured`; a file for `single_file` — and if the
path does not end in `.csv` under that profile, `out.csv` is appended to it.

The **parent** must exist; the directory itself need not. Defaults to the working directory resolved
to an absolute path, not to the literal `.` — see {doc}`../configuration/options`.

A path ending in `.tar.gz` switches the archive flag on and the suffix is stripped, so
`-o results.tar.gz` and `-o results -z` mean the same thing.

### `--out-profile` / `-P`

`regular` (default), `single_file`, or `structured`, case-insensitively. {doc}`../../guides/user/output` says what
each writes.

### `--archive` / `-z` and `--separate-out`

Two independent flags: a compressed sibling of the output, and one CSV per `(report, format)` pair
for the two tables that carry them.

```{note}
Both flags can only be switched **on** from the command line. Present means true; absent means
*nothing said*, not false — otherwise every command line would silently switch off what a
configuration file had turned on. To turn one off, say so where `false` can be written:
`FREEPORTS_ARCHIVE=false`, or `archive: false` in the file.
```

### `--no-download`

Sets `save_pdf` to false. Despite its name it does **not** suppress the download — it suppresses
*keeping* the downloaded file on disk. Absent leaves the field unset; the default, from the defaults
tier, is to keep.

## Batch and configuration

### `--batch` / `-b`

A CSV, one job per row. {doc}`../../guides/user/advanced/batch`.

### `--config`

Use this configuration file instead of searching the three standard tiers.
{doc}`../configuration/config_file`.

## Parallelism

`--workers` / `-j` is the default for **both** levels; `--jobs` and `--pages` override one each.
Each takes a positive integer **or** the word `auto`, and `auto` is a value you can write, not
merely the absence of one — it is how a level is put back to automatic after a configuration file has
pinned it to a number.

Zero and negative values are a typed error naming the option you mistyped, not a generic message
about a concept. {doc}`../../guides/user/advanced/parallelism`.

## Verbosity: `-v` and `-q`
`-v` and `-q` are **counted** and are **independent dials**, not opposed flags: the net of the two
counts is added to the default and clamped, so no combination is an error and `-vv -q` is legal.

| Flags | Level |
|---|---|
| none | warnings |
| `-q` | errors only |
| `-qq` or more | silent |
| `-v` | info |
| `-vv` | debug |
| `-vvv` or more | trace |

```{warning}
`FREEPORTS_VERBOSITY` and the configuration file's `verbosity` key set the same dial, and both
work — but they are inside a configuration that cannot be read until logging is already running, so
they take effect the moment it resolves rather than from the first line. See
{doc}`../configuration/index` for the one consequence.
```

## What the exit status says

A run ends in one of **three** states, and the exit status distinguishes all three. Two would not be
enough, for the same reason `freeports-validate check-grants` needs three
({doc}`freeports-validate`): *everything I read was fine* and *I read everything and it was fine*
are different sentences.

| Exit | What it means |
|---|---|
| `0` | every document of every job was read, and the results were written |
| `1` | the run produced nothing: an illegal configuration, or no job succeeded |
| `3` | the results were written, and **not everything was read** — jobs or documents were skipped |

`3` is deliberately the same number `freeports-validate` uses for the same idea. Two commands of one
suite must not ask you to remember two tables.

### What does *not* raise it

Rows and pages skipped do not. They are contained at their own level, they happen on ordinary runs
by the dozen — a page whose layout a format does not cover, a row whose value will not parse — and
they are already summarised in their own end-of-run event and listed one by one in `.log.csv`
({doc}`../../guides/user/logging`). A run that skipped four hundred rows and read every document it
was given exits `0`.

The line is drawn at **what was not read at all**: a job that failed produced nothing for any of its
documents, and a document that would not open produced nothing at all. Those are pieces of the batch
that are simply absent from the output, and no amount of reading the results would reveal it.

```{note}
A failing job no longer costs the run. A batch of 904 jobs in which two fail writes the other 902
and exits `3`; only a run in which *nothing* was produced exits `1` without writing, because nine
empty tables and nine tables nobody could fill look the same on disk.
```

## `--internal-worker`, the one hidden option
`--internal-worker` exists and is hidden from `--help` on purpose. It is not a user interface but
the channel between two copies of the same binary: a parent running the jobs of a batch in child
processes passes it to itself, naming a request file inside a temporary directory that disappears
when the run ends. Running it by hand makes no sense.
