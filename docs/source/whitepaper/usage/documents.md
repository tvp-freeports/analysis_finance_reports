# Naming a document

A **document specifier** says up to three things at once: where the report comes from, where it
should live on disk, and what to call it in the output.

```text
<url>:<path>:<name>
```

Any of the three may be absent, and the shape is **inferred rather than declared** — there is no
flag saying "this one is a URL". A `http://` or `https://` scheme at the head is what makes a
segment a URL; everything else is a path.

## The forms

| You write | url | path | name |
|---|---|---|---|
| `report.pdf` | — | that path, made absolute | its own text |
| `https://…/r.pdf` | that URL | — | the URL |
| `report.pdf:EURIZON 2023` | — | that path | `EURIZON 2023` |
| `https://…/r.pdf:EURIZON 2023` | that URL | — | `EURIZON 2023` |
| `https://…/r.pdf:local.pdf:` | that URL | `local.pdf` | the URL |
| `https://…/r.pdf:local.pdf:EURIZON 2023` | that URL | `local.pdf` | `EURIZON 2023` |

The two-segment form is ambiguous on its face — `a:b` could be url+name or path+name — and the
scheme resolves it. The last two forms **require** the scheme: without it, a missing URL would
otherwise become a value assembled from nothing, and it is an error instead.

Note the trailing colon in `<url>:<path>:`. It is what distinguishes that form from
`<url>:<name>`, and it is not optional.

Zero segments, or four and more, is an error naming the count. An empty string parses into a
specifier with nothing set — not an error at parse time, only when it is validated as an actual
input.

## Colons inside a value

Wrap the segment in quotes. A URL port, a Windows drive letter, a name containing a colon:

```text
"https://host:8443/r.pdf":"C:\reports\r.pdf":"EURIZON: 2023"
```

A quote only opens a quoted region at the **start** of a segment; anywhere else it stays a literal
character. The whole specifier may also be wrapped in quotes, and the scheme is still detected
underneath.

## Several documents at once

On the command line, repeat `-i` or give several values after one:

```console
$ freeports -i "a.pdf:2023" "b.pdf:2024" -f EURIZON-EN23 …
```

From the environment and from a batch cell, where there is only one string to work with, the
specifiers are separated by a **pipe**:

```console
$ export FREEPORTS_REPORTS='a.pdf:2023|b.pdf:2024'
```

## Directories

A path that is a directory **expands to every PDF in it**, and each expanded document is named
`<name>/<file name>` — so one specifier can name a whole folder and still produce distinguishable
rows:

```console
$ freeports -i "~/reports/2024:2024" -f EURIZON-EN24 …
```

gives documents called `2024/eurizon.pdf`, `2024/amundi.pdf`, and so on.

## The name is not decoration

It is the value that appears in the **`Report` column** of every output table, and it is how rows
from two documents are told apart afterwards. Left to default, it is the whole absolute path — which
works, and is unpleasant to read in a spreadsheet, and will bite you the first time you use
`--separate-out`, since the name becomes part of the file name.

## Downloading, and `save_pdf`

Downloaded documents are **kept** by default. `--no-download` does not suppress the download; it
suppresses keeping the file. The behaviour is a small matrix, and it is worth reading because the
two halves genuinely differ in intent — with saving on, a path is *where to put it*; with saving
off, a path is *where to look first*.

### With `save_pdf` on (the default)

| You gave | What happens |
|---|---|
| url only | downloaded and written to `report.pdf` **in the working directory** |
| url + an existing directory | downloaded to `<dir>/report.pdf` |
| url + an existing file | downloaded to that file |
| url + a non-existent file whose parent exists | downloaded there |
| url + a non-existent file whose parent does **not** exist | **error**, naming the parent |
| url + a non-existent path with no extension | **error**: treated as a directory that should have existed |
| path only, existing file | used as is, nothing downloaded |
| path only, existing directory | expands to the PDFs in it |
| path only, non-existent | **error**, naming the path |

A path with no extension that has never been seen on disk is treated as a *directory*, not as a file
to download. That is what lets "you named a missing directory" be told apart from "you named a
missing file in a directory that exists".

### With `save_pdf` off

| You gave | What happens |
|---|---|
| url only | downloaded, kept nowhere |
| url + a valid PDF path | that file is used |
| url + a directory | expands to the PDFs in it |
| url + an invalid path | **warning**, and the run falls back to the URL |

With saving off, an unusable path is never fatal: there is a URL to fall back to, and falling back
with a warning is more useful than refusing. With saving on there is nothing to fall back to, so
the same situation is an error.
