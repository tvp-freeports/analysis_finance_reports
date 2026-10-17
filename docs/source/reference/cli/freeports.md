# The `freeports` command

Every option, what it sets, and what happens when it is left out. The same settings are reachable
from three other sources — see {doc}`../configuration/index` — and this page is the command-line view of
them.

`freeports` has one subcommand, `classify`, which reports the format of documents without
extracting anything: {doc}`freeports-classify`. Everything on this page is the command without it.

## Every option at a glance
```text
freeports  -i <document>…  -f <format>  -F <formats-repo>  -I <input-db>  -T <list>…
           [-o <path>] [-P <profile>] [-z] [--separate-out] [--no-download]
           [-b <batch.csv>] [--config <file>] [-j <n>|--jobs <n>|--pages <n>] [-v…|-q…]
```

| Option | Sets | Absent means |
|---|---|---|
| `--input`, `--report`, `-i` | the documents | no document from this source |
| `--format`, `-f` | the format | detect it from the documents, or the job fails |
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
`FINECO-EN23@LUX`, `MEDIOLANUM-IT24.A`. It is optional: left out, each job settles its format from
its own documents. {doc}`../design/document-classification` explains why the run works this way;
this section is what it does.

#### Where the format comes from

A job settles its format after its documents are resolved and before the format's recipe is loaded,
from three sources in a fixed priority:

1. **the configuration** — `-f`, or `format` from any other source ({doc}`../configuration/options`);
2. **the document classification** — the repository's `metadata/document_classify.yaml` and
   Python module ({doc}`../formats/document-classify-yaml`,
   {doc}`../formats/document-classify-python`), applied to the first pages of each document;
3. **the URL** — the repository's `url_mapping.csv`, applied to the URL of each document that has
   one: the longest matching prefix wins, then the row with the more specific name, then the row
   that comes first. A row may give a partial name ({doc}`../formats/names`), which narrows the
   format without settling it; another source then has to complete it.

The classification **runs even when the format is configured**: that is how the run can tell you
that the format you gave is not the one the document looks like. It costs roughly 0.1–0.2 s per job.

The two document sources are combined exactly as `freeports classify` combines them
({doc}`freeports-classify`, "How the two sources combine"): two names agree when some format of
the repository falls under both, and the result is the most precise name both allow. The documents
of one job are combined source by source first, then the two sources compared.

The job runs only with a **complete** format. A partial one — `MEDIOLANUM-IT24.*`, say — fails the
job, and the error lists the formats it still allows; run `freeports classify` on the document, or
see {doc}`../formats/names`, to choose one, then give it with `-f`.

#### When the sources cross

Every crossing is logged once per job, at the level below. The `WARN` lines are not something a
setting can switch off, and since `.log.csv` always records `WARN` and above, they are there even
under `-qq`.

| What happens | Logged | The job |
|---|---|---|
| a format is configured, and the sources that answer agree with it | `DEBUG` *configured format confirmed by the document* | runs with the configured format |
| a format is configured, and the classification disagrees with it | `WARN` *the configured format differs from the one the document classification finds; the configured one is used* | runs with the configured format |
| a format is configured, and the URL disagrees with it | `WARN` *… differs from the one the report url maps to; the configured one is used* | runs with the configured format |
| the classification and the URL disagree with each other | `ERROR` *the algorithm and the url disagree on the format* | runs with the configured format if there is one, otherwise with the classification's |
| no format configured, and the sources settle a complete one | `INFO` *format detected*, with `by` = `algorithm`, `url` or `algorithm and url` | runs with that format |
| no format configured, and only a partial one is settled | the job's error, listing the candidates | fails |
| no format configured, and no source says anything | the job's error: *no format was configured, and neither the document classification nor the report url determines one* | fails |
| the documents of the job disagree with each other on one source | the job's error, naming what that source said of each document by its label | fails, configured format or not |
| the classification recognises a document | `INFO` *document classified as '…'* in that document's span, with the page and the step or pipeline; *… by exclusion* when the step is the unconditional *otherwise* of a block | — |

When both sources disagree with a configured format, both `WARN` lines are written: they are two
different facts.

Classification can also **break**, and what that costs depends on whether there is a format to fall
back on:

| What breaks | With a configured format | Without one |
|---|---|---|
| the repository's classification will not load (an invalid `document_classify.yaml`, a module that will not import, a rule refused at load time) | `ERROR` *… could not be loaded, the configured format is used unchecked*; the job runs, checked against neither source | the job fails with the load error |
| a document's classification fails at run time (a page that will not convert, a Python pipeline giving two names, a name that does not exist) | `ERROR` in that document's span; its classification counts as nothing, its URL still counts, and the job runs | the job fails, naming the document |
| `url_mapping.csv` cannot be read, or a row is refused | `WARN` *cannot detect format from this report url*; the URL counts as nothing | the same |

The run never falls back on the URL alone when the classification is broken: an author's rules that
fail are not evidence for anything else.

A job that cannot settle its format costs **that job**. In a batch the other rows run and the run
exits `3`; a run in which no job succeeds exits `1` ([below](#what-the-exit-status-says)).

```{note}
Without a configured format, the events of settling it are logged under a bare `job` span —
`job/document[<name>]` in the `Activity` column of `.log.csv` — because the format is not known
yet. From the moment it is settled, events are under `job[<format>]` as usual. With `-f`, every
event is under `job[<format>]` from the start.
```

## Where things live

### `--formats-directory` / `--repo` / `-F` / `-r`

The formats repository. From the environment it is `FREEPORTS_FORMATS_REPO_PATH`, and
`freeports-dev` and `freeports-validate` read the same variable and accept the same four spellings
on their command lines; see {doc}`index`.

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
