# A first run, argument by argument

Assuming {doc}`installation` is done and the two {doc}`inputs` exist.

```console
$ freeports \
    --input report.pdf \
    --format EURIZON-EN23 \
    --repo ~/work/formats-repo \
    --db-directory ~/work/input-db \
    --target-list controversial_weapons \
    --out ./results
```

Line by line:

`--input report.pdf`
: the document. A bare path here means *this local file*, made absolute, and its own text becomes
  the document's **name** — the value that will appear in the `Report` column of the output. Give it
  a shorter one with `report.pdf:EURIZON 2023`, which you will want as soon as there are two
  documents. The full grammar is in {doc}`documents`.

`--format EURIZON-EN23`
: which recipe to read it with. Omissible **only** when the document came from a URL the formats
  repository's mapping recognises; a local path carries no evidence of who published it. Naming a
  format the repository does not have is an error naming the format.

`--repo ~/work/formats-repo`
: the formats repository. Also `--formats-directory`, `-F`, `-r`, or `FREEPORTS_FORMATS_REPO_PATH`.

`--db-directory ~/work/input-db`
: the input database. Also `-I`, or `FREEPORTS_INPUT_DB_PATH`.

`--target-list controversial_weapons`
: which list of companies to look for. Repeatable. **Without it the run stops immediately**, before
  opening the PDF — the presence check runs first on purpose, so a missing input fails in a second
  rather than after four minutes of PDF.

`--out ./results`
: where the tables go. A directory for the default profile. Defaults to the working directory, so
  omitting it is legal and rarely what you want.

## What you get

```console
$ ls results/
assets_managers.csv     funds_change_name.csv          funds.csv
funds_assets.csv        funds_esg_indicators.csv       funds_sfdr_classification.csv
investments.csv         investments_add_infos.yaml     investments_managers_to_funds.csv
.log.csv
```

Nine tables and an audit trail. Most runs care about `investments.csv` — one row per holding — and
the rest describe the funds those holdings belong to. Every CSV has its header even when it has no
rows, so *"nothing found"* and *"no such table"* stay distinguishable. {doc}`output` is the full
list and the rules.

`.log.csv` is not an output table: it is the extraction's own record of what it skipped and where.
It lands **next to the output** because it is a product of the run. {doc}`logging` explains how to
read it.

## Adding a second document

```console
$ freeports -i "a.pdf:2023" -i "b.pdf:2024" -f EURIZON-EN23 …
```

Two jobs, one run, **one** set of tables, with the `Report` column telling the rows apart. This is
not the same as two separate runs concatenated: in one invocation the two documents share promise
resolution and fund deduplication. See {doc}`../design/multidocument`.

## When it does not work

| Symptom | Usually |
|---|---|
| stops at once, says a target list is missing | `--target-list` not given — the first check |
| runs, finds nothing, writes header-only tables | the format is right but the companies are not in the named list, or the list is empty |
| a format error naming a format | the name is not in the repository's `metadata/formats.csv` |
| holdings attributed to the wrong company | the input database's order, not the extraction — see {doc}`inputs` |

Raise the verbosity before guessing: `-v` narrates the run, `-vv` and `-vvv` more. {doc}`logging`.
