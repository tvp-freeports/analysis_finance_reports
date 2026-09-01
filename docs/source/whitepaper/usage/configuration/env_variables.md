# Environment variables

Stronger than the configuration file, weaker than the command line. Per-option detail is in
{doc}`options`.

## Variable to option

| Variable | Option |
|---|---|
| `FREEPORTS_URL`, `FREEPORTS_PDF` | `reports`, in two halves |
| `FREEPORTS_REPORTS` | `reports`, several specifiers separated by `\|` |
| `FREEPORTS_TARGET_LIST` | `target_lists` — **one** element, the whole raw value |
| `FREEPORTS_FORMAT` | `format` |
| `FREEPORTS_FORMATS_REPO_PATH` | `formats_repo_path` |
| `FREEPORTS_INPUT_DB_PATH` | `input_db_path` |
| `FREEPORTS_OUT_PATH` | `out_path` |
| `FREEPORTS_OUT_PROFILE` | `out_profile` |
| `FREEPORTS_SEPARATE_OUT` | `separate_out` |
| `FREEPORTS_ARCHIVE` | `compressed` |
| `FREEPORTS_SAVE_PDF` | `save_pdf` |
| `FREEPORTS_N_WORKERS` | `n_workers` |
| `FREEPORTS_PARALLELISM_JOBS` | `parallelism_jobs` |
| `FREEPORTS_PARALLELISM_PAGES` | `parallelism_pages` |
| `FREEPORTS_BATCH_FILE` | `batch_file` |
| `FREEPORTS_CONFIG_FILE` | `config_file` |
| `FREEPORTS_VERBOSITY` | `verbosity` — *parsed and merged, but not applied; see* {doc}`index` |

Two further prefixes exist and belong to the other two commands, not to the engine:
`FREEPORTS_DEV_…` for `freeports-dev` and `FREEPORTS_VALIDATE_…` for `freeports-validate`. They
mirror the `dev:` and `validate:` sections of the configuration file one for one, and are documented
in {doc}`../../formats/configuration`.

## Rules of the environment
**An absent variable leaves its option unset**, and is never an error.

**Setting both `FREEPORTS_REPORTS` and one of the singular variables is an error.** One of the two
would have to win silently, and an override nobody can see is worse than a refusal.

**A variable whose value is not valid UTF-8 is ignored with a warning**, rather than failing the
run — it is almost always an accident of the shell rather than an intention.

## The grammars the variables accept
**Booleans** — `FREEPORTS_SEPARATE_OUT`, `FREEPORTS_ARCHIVE`, `FREEPORTS_SAVE_PDF` — accept, case
insensitively:

| True | False |
|---|---|
| `true`, `yes`, `1`, `y`, `t` | `false`, `no`, `0`, `n`, `f` |

Anything else is a typed error naming the variable. This is deliberately more permissive than YAML's
own booleans: an environment variable is often set by a shell script from a value that came from
somewhere else, and `1` is a perfectly ordinary way to write true there.

**Parallelism** — the three variables share one grammar, a positive integer or `auto`, and differ
only in the name that appears in the error.

**Verbosity** — the variant name, case-insensitively: `silent`, `error`, `warn`, `info`, `debug`,
`trace`.

**Profile** — `regular`, `single_file`, `structured`, case-insensitively.

**Several documents** in `FREEPORTS_REPORTS` are separated by a pipe, `|` — the same separator a
batch cell uses, and one constant in the code rather than two literals that could drift apart.

## `FREEPORTS_TARGET_LIST` is never split
`FREEPORTS_TARGET_LIST` becomes a **one-element** list holding the whole raw value, whatever is in
it. A name containing a comma, or a pipe, still names one list. To use several lists, use a source
that has a list type — the command line, or the YAML file.
