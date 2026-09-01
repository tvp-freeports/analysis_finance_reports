# The canonical options

Sixteen settings. Each has one meaning and up to four spellings — one per source. This page is the
per-option reference; the four pages after it are the per-source views of the same table.

Every card has the same fields, in the same order, so the page can be consulted rather than read. A
dash means *this source cannot set this option*.

## Summary

| Option | Command line | Environment | YAML | Batch |
|---|---|---|---|---|
| [reports](#reports) | `-i` | `FREEPORTS_URL`, `FREEPORTS_PDF`, `FREEPORTS_REPORTS` | `url`, `pdf`, `reports` | `url`, `pdf` |
| [target_lists](#target-lists) | `-T` | `FREEPORTS_TARGET_LIST` | `target_lists` | `target list` |
| [format](#format) | `-f` | `FREEPORTS_FORMAT` | `format` | `format` |
| [formats_repo_path](#formats-repo-path) | `-F`, `-r` | `FREEPORTS_FORMATS_REPO_PATH` | `formats_repo` | — |
| [input_db_path](#input-db-path) | `-I` | `FREEPORTS_INPUT_DB_PATH` | `db_path` | — |
| [out_path](#out-path) | `-o` | `FREEPORTS_OUT_PATH` | `out_path` | — |
| [out_profile](#out-profile) | `-P` | `FREEPORTS_OUT_PROFILE` | `out_profile` | — |
| [separate_out](#separate-out) | `--separate-out` | `FREEPORTS_SEPARATE_OUT` | `out_flags.separate_out` | — |
| [compressed](#compressed) | `-z` | `FREEPORTS_ARCHIVE` | `out_flags.archive` | — |
| [save_pdf](#save-pdf) | `--no-download` | `FREEPORTS_SAVE_PDF` | `save_pdf` | `save pdf` |
| [n_workers](#n-workers) | `-j` | `FREEPORTS_N_WORKERS` | `n_workers` | — |
| [parallelism_jobs](#parallelism-jobs) | `--jobs` | `FREEPORTS_PARALLELISM_JOBS` | `parallelism.jobs` | — |
| [parallelism_pages](#parallelism-pages) | `--pages` | `FREEPORTS_PARALLELISM_PAGES` | `parallelism.pages` | — |
| [batch_file](#batch-file) | `-b` | `FREEPORTS_BATCH_FILE` | `batch_file` | — |
| [config_file](#config-file) | `--config` | `FREEPORTS_CONFIG_FILE` | — | — |
| [verbosity](#verbosity) | `-v`, `-q` | `FREEPORTS_VERBOSITY` | `verbosity` | — |

---

(reports)=
## `reports`

| | |
|---|---|
| **Command line** | `--input`, `--report`, `-i` — repeatable, several values allowed |
| **Environment** | `FREEPORTS_URL` + `FREEPORTS_PDF` (singular), or `FREEPORTS_REPORTS` (plural, `\|`-separated) |
| **YAML** | `url` + `pdf` (singular), or `reports` (a list) |
| **Batch column** | `url`, `pdf` |
| **Type** | a list of document specifiers, `<url>:<path>:<name>` |
| **Default** | the empty list |
| **Validation** | each specifier parsed by the grammar; then paths and URLs checked against `save_pdf` |
| **Example** | `-i "https://x/r.pdf:local.pdf:2023"` |

Giving both the singular and the plural form **on the same source** is an error: one of the two
would have to win silently. Different sources may of course use different forms — the merge then
applies normally, and the stronger source's list replaces the weaker's entirely.

The full grammar, and the behaviour of directories and of `save_pdf` case by case, is in
{doc}`../documents`.

---

(target-lists)=
## `target_lists`

| | |
|---|---|
| **Command line** | `--target-list`, `-T` — repeatable, several values allowed |
| **Environment** | `FREEPORTS_TARGET_LIST` — **one** element, the whole raw value, never split |
| **YAML** | `target_lists` — a list |
| **Batch column** | `target list` — one element, taken whole |
| **Type** | list of names |
| **Default** | **none** — deliberately unset |
| **Validation** | presence, checked **first of all**; absent from every source is a hard error |
| **Example** | `-T controversial_weapons -T coal` |

The one structural field the defaults deliberately do **not** fill in. Its absence has to stay
visible so validation can name it: if the defaults supplied an empty list, a run with no target list
would silently search for nothing and report success over an empty table.

The environment and batch forms never split their value, so a list name containing a comma or a
pipe still works.

---

(format)=
## `format`

| | |
|---|---|
| **Command line** | `--format`, `-f` |
| **Environment** | `FREEPORTS_FORMAT` |
| **YAML** | `format` |
| **Batch column** | `format` |
| **Type** | string |
| **Default** | none — inferred, or an error |
| **Validation** | inference from document URLs via the repository's `url_mapping.csv` |
| **Example** | `-f EURIZON-EN23` |

Left unset, the engine infers it from the URL of each document; the first matching prefix in the
mapping wins, so the order of that file is meaningful. Two documents inferring **different** formats
is an error. An explicit format that **disagrees** with a successful inference is a warning, and the
explicit one is used.

There is no inference for a local file: a path carries no evidence of who published it.

---

(formats-repo-path)=
## `formats_repo_path`

| | |
|---|---|
| **Command line** | `--formats-directory`, `--repo`, `-F`, `-r` |
| **Environment** | `FREEPORTS_FORMATS_REPO_PATH` |
| **YAML** | `formats_repo` |
| **Batch column** | — |
| **Type** | path |
| **Default** | none |
| **Validation** | read when formats are loaded; a repository without `metadata/formats.csv` fails there |
| **Example** | `-r ~/work/formats-repo` |

```{warning}
`freeports-dev` and `freeports-validate` read **`FREEPORTS_FORMATS_REPO`** — a different variable
name for the same thing. Export both. See {doc}`../../formats/tooling`.
```

---

(input-db-path)=
## `input_db_path`

| | |
|---|---|
| **Command line** | `--db-directory`, `-I` |
| **Environment** | `FREEPORTS_INPUT_DB_PATH` |
| **YAML** | `db_path` |
| **Batch column** | — |
| **Type** | path |
| **Default** | none |
| **Validation** | read when the target lists are loaded |
| **Example** | `-I ~/work/input-db` |

---

(out-path)=
## `out_path`

| | |
|---|---|
| **Command line** | `--out`, `-o` |
| **Environment** | `FREEPORTS_OUT_PATH` |
| **YAML** | `out_path` |
| **Batch column** | — |
| **Type** | path |
| **Default** | the working directory, **resolved to an absolute path** |
| **Validation** | the **parent** must exist; the directory itself need not |
| **Example** | `-o ./results` |

The default is the absolute working directory rather than the literal `.`, and that is not a
detail: in Rust the parent of `.` is the empty path, which never exists, so a literal would make
the parent check fail for every run that did not pass `--out` — a default that breaks the default
case.

A path ending in `.tar.gz` switches [`compressed`](#compressed) on and has the suffix stripped.
Under the `single_file` profile a path not ending in `.csv` gets `out.csv` appended.

---

(out-profile)=
## `out_profile`

| | |
|---|---|
| **Command line** | `--out-profile`, `-P` |
| **Environment** | `FREEPORTS_OUT_PROFILE` |
| **YAML** | `out_profile` |
| **Batch column** | — |
| **Type** | one of `regular`, `single_file`, `structured` |
| **Default** | `regular` |
| **Validation** | the name, case-insensitively; anything else is an error listing the three |
| **Example** | `-P structured`, `FREEPORTS_OUT_PROFILE=single_file` |

The same three names on every source, so a run does not change meaning when its profile moves from
a flag into a file. {doc}`../output` says what each writes.

---

(separate-out)=
## `separate_out`

| | |
|---|---|
| **Command line** | `--separate-out` — present means true, absent means *nothing said* |
| **Environment** | `FREEPORTS_SEPARATE_OUT` — `true`/`yes`/`1`/`y`/`t`, `false`/`no`/`0`/`n`/`f` |
| **YAML** | `out_flags.separate_out` — a real YAML boolean |
| **Batch column** | — |
| **Type** | boolean |
| **Default** | `false` |
| **Validation** | the boolean grammar of its source |
| **Example** | `out_flags:\n  separate_out: true` |

An **independent field**, not half of a group: a source setting this one leaves
[`compressed`](#compressed) to whatever another source said.

---

(compressed)=
## `compressed`

| | |
|---|---|
| **Command line** | `--archive`, `-z` — present means true, absent means *nothing said* |
| **Environment** | `FREEPORTS_ARCHIVE` — same boolean grammar |
| **YAML** | `out_flags.archive` — a real YAML boolean |
| **Batch column** | — |
| **Type** | boolean |
| **Default** | `false` |
| **Validation** | the boolean grammar of its source |
| **Example** | `FREEPORTS_ARCHIVE=false` |

Publicly the option is called **archive**, on every source. `compressed` is only its internal field
name, and `out_flags: compressed:` in a file is rejected as an unknown sub-key.

Also switched on implicitly by an `out_path` ending in `.tar.gz`.

---

(save-pdf)=
## `save_pdf`

| | |
|---|---|
| **Command line** | `--no-download` — present sets it **false**; absent means *nothing said* |
| **Environment** | `FREEPORTS_SAVE_PDF` — the boolean grammar |
| **YAML** | `save_pdf` — a real YAML boolean |
| **Batch column** | `save pdf` — literally `true` or `false` |
| **Type** | boolean |
| **Default** | `true` |
| **Validation** | the boolean grammar; then it changes how document paths are validated |
| **Example** | `--no-download`, `save_pdf: false` |

The one option whose command-line spelling is negative, which is why the flag's name misleads:
`--no-download` does not suppress the download, it suppresses **keeping** the downloaded file.

---

(n-workers)=
## `n_workers`

| | |
|---|---|
| **Command line** | `--workers`, `-j` |
| **Environment** | `FREEPORTS_N_WORKERS` |
| **YAML** | `n_workers` |
| **Batch column** | — |
| **Type** | a positive integer, or `auto` |
| **Default** | `auto` |
| **Validation** | zero and negatives are a typed error naming the option |
| **Example** | `-j 4`, `n_workers: auto` |

The **global default of both levels**, not a process count. It is what gives `-j 1` a universal
meaning: one job at a time *and* one page at a time. A level with its own value ignores it.

---

(parallelism-jobs)=
## `parallelism_jobs`

| | |
|---|---|
| **Command line** | `--jobs` — long form only, `-j` being taken by the global default |
| **Environment** | `FREEPORTS_PARALLELISM_JOBS` |
| **YAML** | `parallelism.jobs` |
| **Batch column** | — |
| **Type** | a positive integer, or `auto` |
| **Default** | unset — inherits `n_workers` |
| **Validation** | as `n_workers`, with this option's name in the error |
| **Example** | `--jobs 2` |

Left unset on purpose by the defaults tier: a level nobody touched has no value of its own and
inherits the global one. Were the defaults to fill it in, the global default could no longer reach
it.

---

(parallelism-pages)=
## `parallelism_pages`

| | |
|---|---|
| **Command line** | `--pages` |
| **Environment** | `FREEPORTS_PARALLELISM_PAGES` |
| **YAML** | `parallelism.pages` |
| **Batch column** | — |
| **Type** | a positive integer, or `auto` |
| **Default** | unset — inherits `n_workers` |
| **Validation** | as `n_workers` |
| **Example** | `parallelism:\n  pages: auto` |

---

(batch-file)=
## `batch_file`

| | |
|---|---|
| **Command line** | `--batch`, `-b` |
| **Environment** | `FREEPORTS_BATCH_FILE` |
| **YAML** | `batch_file` |
| **Batch column** | — |
| **Type** | path |
| **Default** | none — not in batch mode |
| **Validation** | the CSV is read and its columns checked; an unknown column is an error |
| **Example** | `-b jobs.csv` |

{doc}`../batch`.

---

(config-file)=
## `config_file`

| | |
|---|---|
| **Command line** | `--config` |
| **Environment** | `FREEPORTS_CONFIG_FILE` |
| **YAML** | — |
| **Batch column** | — |
| **Type** | path |
| **Default** | none — the three tiers are searched instead |
| **Validation** | the file is parsed; unknown keys are errors |
| **Example** | `--config ./ci-freeports.yaml` |

No YAML spelling, for the obvious reason: a configuration file naming the configuration file to read
would be a chicken with no egg. {doc}`config_file` lists the tiers searched when this is unset.

---

(verbosity)=
## `verbosity`

| | |
|---|---|
| **Command line** | `-v` and `-q`, both **counted** and independent |
| **Environment** | `FREEPORTS_VERBOSITY` — by variant name, case-insensitively |
| **YAML** | `verbosity` — same names |
| **Batch column** | — |
| **Type** | one of `silent`, `error`, `warn`, `info`, `debug`, `trace` |
| **Default** | `warn` |
| **Validation** | the name; the counts are clamped and can never be out of range |
| **Example** | `-vv`, `FREEPORTS_VERBOSITY=trace` |

```{warning}
The environment and YAML forms are parsed, validated and merged but **do not reach the parent
process's logging** — see {doc}`index`. Today only `-v` and `-q` change what you see.
```
