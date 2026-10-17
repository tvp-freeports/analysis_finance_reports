# `freeports classify`

`freeports classify` answers one question: *which format is this document?* It reads the formats
repository's document classification ({doc}`../formats/document-classify-yaml`,
{doc}`../formats/document-classify-python`) and its `url_mapping.csv`, applies them to the documents
it is given, and prints what they say. It extracts nothing, writes no output tables, and leaves no
`.log.csv` behind.

```console
$ freeports classify -F ../formats -i a.pdf
/home/me/reports/a.pdf: MEDIOLANUM-IT24.A
```

It is a **subcommand**: `classify` comes right after `freeports`, before any option. Without it,
`freeports` is the extraction command of {doc}`freeports`, unchanged.

## Every option at a glance

```text
freeports classify  -i <document>…  -F <formats-repo>  [--config <file>] [--no-download] [-v…|-q…]
```

| Option | Sets | Absent means |
|---|---|---|
| `--input`, `--report`, `-i` | the documents | no document from this source |
| `--formats-directory`, `--repo`, `-F`, `-r` | the formats repository | unset — the command fails |
| `--config` | use this configuration file | search the three tiers |
| `--no-download` | do not **keep** a downloaded PDF | *nothing said* — the default is to keep |
| `-v`, `-q` | verbosity, counted | warnings only |

Each option means exactly what it means for the extraction command, with the same spellings and
the same grammar — the details are in {doc}`freeports`, and are not repeated here.

The options of the run that have nothing to do with recognising a format — `-f`, `-T`, `-I`, `-o`,
`-P`, `-z`, `--separate-out`, `-b`, `-j`, `--jobs`, `--pages` — are not offered: `classify --help`
does not list them, and giving one is a command-line error rather than an option quietly ignored.
The same holds the other way round: `freeports -f X classify …` is refused. And since a subcommand
is only recognised in first position, `freeports -i a.pdf classify` is **not** a classification —
it is a run with two documents, `a.pdf` and a file called `classify`.

## Where its settings come from

`classify` resolves its configuration exactly as the run does: **command line, then environment,
then configuration file, then a default** ({doc}`../configuration/index`). It reads five settings
from those tiers and no others:

| Setting | Command line | Environment |
|---|---|---|
| `reports` | `-i` | `FREEPORTS_REPORTS`, or `FREEPORTS_URL` / `FREEPORTS_PDF` |
| `formats_repo_path` | `-F` | `FREEPORTS_FORMATS_REPO_PATH` |
| `save_pdf` | `--no-download` | `FREEPORTS_SAVE_PDF` |
| `verbosity` | `-v`, `-q` | `FREEPORTS_VERBOSITY` |
| `config_file` | `--config` | `FREEPORTS_CONFIG_FILE` |

Every other setting is **ignored without error**, wherever it is set. A configuration file written
for the run, with its `format`, `target_lists` and `out_path`, works unchanged for `classify`; so does
a shell that exports `FREEPORTS_FORMAT` or `FREEPORTS_BATCH_FILE`. In particular a `format` set
somewhere does not influence the answer — the point of the command is to find the format, not to be
told it. There are no settings specific to `classify`.

Two settings must end up with a value: at least one document, and the formats repository. Missing
either is a configuration error (exit `1`).

## Documents

Documents are given with `-i`, in the same `<url>:<path>:<name>` grammar as the run
({doc}`../../guides/user/documents`). Several may be given, and a directory expands to the PDFs in
it.

**A document given by URL is fetched as in the run, and kept as in the run.** `save_pdf` is on by
default, so a URL with no path is downloaded to `report.pdf` in the working directory and stays
there; a URL with a path is downloaded to that path. `--no-download` (or `save_pdf: false`) turns
this off: the download then goes to a temporary file, which is removed before the command ends.

```{warning}
When the path a document would be saved to **already exists as a file**, that file is read and
nothing is downloaded. A `report.pdf` left in the working directory by an earlier run is therefore
classified in place of the URL, and two URL-only documents in one command both end up reading the
same `./report.pdf`. Give each URL its own path (`-i "https://…/a.pdf:a.pdf"`), or pass
`--no-download`.
```

## What it prints

The answer goes to **standard output**, at every verbosity — even `-qq` prints it, since it is the
product of the command, not a diagnostic. The log goes to standard error, governed by `-v` and `-q`
as in the run ({doc}`freeports`).

### One line per document

Each document gets one line, labelled with its **name** — by default the name is the absolute path
of a local file, or the URL; a name given in the specifier replaces it.

```text
/home/me/a.pdf: MEDIOLANUM-IT24.A
/home/me/b.pdf: MEDIOLANUM-IT24.A (by exclusion)
/home/me/c.pdf: partial MEDIOLANUM-IT24.* — candidates: MEDIOLANUM-IT24.A, MEDIOLANUM-IT24.B
/home/me/d.pdf: not classified
/home/me/e.pdf: error: <why the document could not be read>
```

| Line | Means |
|---|---|
| `<format>` | a complete format |
| `<format> (by exclusion)` | a complete format decided by an unconditional step: the document never said it, it is the *otherwise* of a block ({doc}`../formats/document-classify-yaml`, "The unconditional step") |
| `partial <pattern> — candidates: …` | only a partial name could be established; the candidates are the formats of the repository it still allows, in `formats.csv` order |
| `not classified` | neither the classification nor the URL says anything |
| `error: …` | the document could not be read: the download failed, the file would not open, or the classification failed at run time on it |

A partial is not a failure: it is exactly how far the repository's rules can see, and the candidate
list is where to start trying formats by hand — see {doc}`../formats/names`, "I have a document:
which format do I use?". A partial with a single candidate is still printed as a partial; the
command never picks a format the sources did not name.

The line is the result of **both sources for that document** — the classification and, if the
document has a URL, `url_mapping.csv` — combined as described [below](#how-the-two-sources-combine).

### The `format:` line

With more than one document, a last line gives the format of the set, in the same forms:

```text
/home/me/a.pdf: MEDIOLANUM-IT24.*
/home/me/b.pdf: MEDIOLANUM-IT24.B
format: MEDIOLANUM-IT24.B
```

It is the format of the documents that could be read: a document on an `error:` line is left out
of it (and a warning on standard error says so). It is absent when the documents disagree with each
other, and when none of them could be read.

### The detail, with `-v`

From `-v` on, each readable document's line is followed by indented lines saying what each source
found:

```text
/home/me/a.pdf: MEDIOLANUM-IT24.A
  algorithm: MEDIOLANUM-IT24.A — page 1, YAML entry 0, step `MEDIOLANUM-IT24.*` [0, 1] (by exclusion)
  url: MEDIOLANUM — https://example.com/mediolanum/a.pdf
  sources: agree
```

| Line | Forms |
|---|---|
| `algorithm:` | a complete format, then where it was decided: the page as written in the file (`-1` is the last page), the index of the deciding entry, and either the Python pipeline (``Python entry 2, pipeline `mediolanum` ``) or the YAML step key with its position — its index among the entry's steps, then among each level's children; `partial <pattern>` (a partial carries no page); `not classified` |
| `url:` | `<pattern> — <url>`, or `not mapped — <url>` when no row of `url_mapping.csv` matches. Absent when the document has no URL |
| `sources:` | `agree`, `algorithm only`, `url only`, `disagree, the algorithm wins`, or `neither` |

A document on an `error:` line has no detail: the message is already on its line. `-vv` and `-vvv`
add nothing to standard output, only to the log.

## How the two sources combine

The **algorithm** is the repository's document classification; the **URL** is `url_mapping.csv`
applied to the document's URL. Either may give a complete format, a partial name, or nothing.

Two names **agree** when some format of the repository falls under both. When they agree the result
is the most specific name both allow, which may be more specific than either:

| Algorithm | URL | Result |
|---|---|---|
| `MEDIOLANUM-IT24.A` | — | `MEDIOLANUM-IT24.A`, algorithm only |
| — | `MEDIOLANUM-IT24.B` | `MEDIOLANUM-IT24.B`, URL only |
| `MEDIOLANUM-IT24.A` | `MEDIOLANUM` | `MEDIOLANUM-IT24.A`: the complete name refines the partial |
| `MEDIOLANUM-IT24.*` | `MEDIOLANUM-IT24.B` | `MEDIOLANUM-IT24.B`: the URL completes the algorithm's partial |
| `X-IT23@LUX.*` | `X-IT23@*.B` | `X-IT23@LUX.B` if the repository has it; otherwise they disagree |
| `MEDIOLANUM-IT24.A` | `EURIZON-EN23` | `MEDIOLANUM-IT24.A`: they disagree, and **the algorithm wins** |

A disagreement between the two sources is **not** a failure of the command. The algorithm read the
document; the URL only says where it came from, and a mapping that has fallen behind is the likelier
mistake. So the algorithm's answer is printed, an `ERROR` naming both patterns goes to standard
error, and the exit status is still `0`. The `ERROR` is the signal that `url_mapping.csv` — or the
classification — needs looking at.

### Several documents

Several documents given together are taken to be **one input of one format**, as a job of the run
is — and the run combines them by the same two steps ({doc}`freeports`, "Where the format comes
from"). They are combined **source by source first, and the sources are compared afterwards**:

1. the algorithm's names of all the documents are combined with each other, and so are the URL's.
   A document with nothing to say for a source is neutral for it. If the names of one source leave
   no format of the repository that fits them all, **the documents disagree**: the command fails
   (exit `1`), and the error lists what that source said of each document, naming each by the
   label of its line (`document /home/me/a.pdf is …, document /home/me/b.pdf is …`);
2. the two combined names are then compared as for a single document: agreeing, they give their
   most specific common name; disagreeing, the algorithm wins with an `ERROR`.

The order of the documents does not change the outcome. The order of the two steps can: if one
document is recognised only by the algorithm as `X` and another only by its URL as an incompatible
`Y`, the documents do not disagree with each other — each source has a single opinion — but the
two sources disagree, so `X` wins with an `ERROR` and the command exits `0`.

When the documents disagree, their own lines are still printed — they are what tells you which
document is the odd one out — and only the `format:` line is missing.

## What the exit status says

The same three codes as the run, with the same meanings ({doc}`freeports`):

| Exit | What it means |
|---|---|
| `0` | every document was read and its outcome printed — including `not classified`, a partial, and an algorithm overruling the URL |
| `1` | nothing usable: an illegal configuration (an invalid specifier, a path that does not exist, no document, no formats repository), a classification that will not load, documents that disagree with each other, or no document readable at all |
| `3` | some documents were read and some were not — their lines say `error:` |

`not classified` exits `0` on purpose. The command did its job: it read the document and reports
that the repository's rules do not recognise it. Whether that is a problem is for the caller to
decide from the printed line, not from the status.

A classification that will not load — an error in `document_classify.yaml` or in the Python module
({doc}`../formats/document-classify-yaml`, "What is refused when the file is loaded") — fails
before any document is read, and prints nothing on standard output.

## Logging

The log follows the same rules as the run's ({doc}`../../guides/user/logging`), with one difference:
`classify` has no output directory, so it never writes a `.log.csv`. At `-vvv` the
`freeports.log.jsonl` trace goes to the working directory, as it does for the run.
