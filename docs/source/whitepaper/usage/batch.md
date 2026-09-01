# Batch mode

`--batch` / `-b` takes a CSV, **one job per row**. It is the way to describe many extractions once
and run them as a unit, and it is also where the highest-precedence configuration source lives.

## The batch columns
Recognised case-insensitively, with spaces and underscores equivalent — `save pdf`, `Save_PDF` and
`SAVE PDF` are one column.

| Column | Sets |
|---|---|
| `url` | the report's URL |
| `pdf` | one or more document specifiers, pipe-separated |
| `format` | the format for this row |
| `save pdf` | whether to keep a downloaded PDF: `true` or `false` |
| `target list` | the target lists for this row, taken **whole** |

```text
url,pdf,format,target list
https://example.org/eurizon-2023.pdf,,EURIZON-EN23,controversial_weapons
,~/reports/amundi.pdf:AMUNDI 2024,AMUNDI-EN24,coal
,"a.pdf:A|b.pdf:B",EURIZON-EN23,controversial_weapons
```

An **unrecognised column is an error**, not something ignored. A misspelled column that is silently
dropped configures nothing and says nothing about it, and the user is left believing a setting took
effect — the same principle as the configuration file's unknown keys.

A header with no data rows produces **no jobs**, which is not an error.

## How `url` and `pdf` combine

The `pdf` cell is always split on the pipe, even when it holds a single element, and what happens
next depends on how many elements come out:

- **one element** is treated as the *singular* specifier, and a `url` in the same row overrides its
  URL field. The two can therefore never conflict — a common row is a URL in one column and a
  destination path in the other;
- **several elements** are a genuine plural, and a `url` alongside them **is a conflict**, exactly
  as in the other configuration sources: one of the two would have to win silently.

Rows are numbered from one, the header not counting, and every error names its row.

## Precedence: the row wins

A batch row has the **highest precedence of any configuration source** — above the command line,
above the environment, above the file.

That inversion is deliberate. In batch mode the command line describes the *run* and the row
describes *this job*, and the more specific statement should be the one that holds. It is what lets
a single invocation set the output path, the repository and the parallelism once, while each row
says only what makes it different.

```console
$ freeports -b jobs.csv -F ~/work/formats-repo -I ~/work/input-db -o ./results -j 4
```

## One run, still one set of tables

Batch mode does not change what is produced. Every job's results accumulate and are written once, at
the end, by the parent process — a batch of ten reports gives one `investments.csv` with a `Report`
column, not ten directories.

This is also why job-level parallelism uses child processes that **send their results back** rather
than each writing its own output: writing per child would have changed the meaning of batch mode.
See {doc}`parallelism` and {doc}`../design/parallelism`.

## Batch mode versus several `-i`

Both run several documents in one invocation, and they are not the same thing:

| | several `-i` | batch |
|---|---|---|
| documents | one job, several documents | one job **per row** |
| format | one, for all of them | per row |
| target lists | one set, for all of them | per row |
| the pages | one pool: every step sees them all | one pool per row, isolated from the others |
| filter data, promises, deduplication | shared across the documents | per row |
| use it when | the files are **pieces of one report** | the files are **separate reports** |

The last two rows are the whole difference. Several `-i` is the multi-document case
({doc}`../design/multidocument`): the pages of all the documents are classified per document and then
scheduled **together**, so a step reading one file produces the filter data a later step over another
file receives. That is what lets a report published as several files — holdings in one, the managers
of those funds in another — be extracted as the single report it is. Batch is genuinely separate jobs
that happen to be run together and reported together; nothing crosses between rows.

Reaching for batch where a bundle was meant produces output that looks right and is incomplete: each
row extracts what its own file contains, and the information that only exists across the files is
simply absent.
