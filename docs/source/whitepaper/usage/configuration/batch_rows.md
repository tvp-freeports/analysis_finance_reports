# Batch rows

The **strongest** source of all: a row overrides the command line, because in batch mode the command
line describes the run and the row describes this job. {doc}`../batch` is how batch mode works; this
page is its configuration view.

## Column to option

Columns are matched case-insensitively, with spaces and underscores equivalent — `save pdf`,
`Save_PDF` and `SAVE PDF` are one column.

| Column | Option |
|---|---|
| `url` | `reports`, contributing the URL |
| `pdf` | `reports`, one or more specifiers separated by `\|` |
| `format` | `format` |
| `save pdf` | `save_pdf` |
| `target list` | `target_lists` — one element, taken whole |

Five columns, against sixteen options. That is the design, not an omission: a batch row says what
makes **this job** different, and everything about the run as a whole — where the output goes, which
repository and database to use, how much parallelism, the output shape — is said once on the command
line or in a file rather than repeated on every row.

## Rules of a batch row
**An unknown column is an error**, not something ignored, and the message names the column. Same
principle as the file's unknown keys.

**Rows are numbered from one**, the header not counting, and every error names its row. These files
are edited in spreadsheets by people who will never see a stack trace, and an error that cannot say
which row is nearly useless.

**A header with no data rows produces no jobs**, which is not an error.

**`save pdf` is stricter here** than in the environment: literally `true` or `false`,
case-insensitively, and nothing else. A batch file is written once and read by a program, not typed
at a shell prompt, so the permissiveness that helps a shell script buys nothing here.

## How `url` and `pdf` combine in a row
The `pdf` cell is always split on the pipe, even when it holds one element:

- **one element** is treated as the *singular* specifier, and a `url` in the same row overrides its
  URL field. The two therefore can never conflict — the common row is a URL in one column and a
  destination path in the other;
- **several elements** are a genuine plural, and a `url` alongside them **is a conflict**, exactly
  as in the file and the environment.

```text
url,pdf,format,target list
https://example.org/r.pdf,local.pdf,EURIZON-EN23,controversial_weapons
,"a.pdf:A|b.pdf:B",EURIZON-EN23,coal
```

The first row downloads to `local.pdf`; the second runs two named documents as one job.
