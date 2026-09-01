# The configuration file

YAML, and the weakest source above the built-in defaults. It is where the settings that do not
change between runs belong — the repository, the database, the lists, the output shape — so that a
command line only has to say what is different this time.

## Key to option

| Key | Option | Note |
|---|---|---|
| `url`, `pdf` | `reports`, in two halves | |
| `reports` | `reports` | a list, each element in the specifier grammar |
| `target_lists` | `target_lists` | a list |
| `format` | `format` | |
| `formats_repo` | `formats_repo_path` | |
| `db_path` | `input_db_path` | |
| `out_path` | `out_path` | |
| `out_profile` | `out_profile` | a profile name, case-insensitively |
| `out_flags` | `separate_out`, `compressed` | a map, only `separate_out` and `archive` |
| `save_pdf` | `save_pdf` | a real YAML boolean |
| `n_workers` | `n_workers` | a positive integer or `auto` |
| `parallelism` | `parallelism_jobs`, `parallelism_pages` | a map, only `jobs` and `pages` |
| `batch_file` | `batch_file` | |
| `verbosity` | `verbosity` | *parsed and merged, but not applied; see* {doc}`index` |

There is no key for `config_file`: a configuration file naming the configuration file to read would
be a chicken with no egg.

## A complete example

```yaml
formats_repo: ~/work/formats-repo
db_path: ~/work/input-db
target_lists:
  - controversial_weapons
  - coal
out_path: ~/work/results
out_profile: regular
out_flags:
  separate_out: true
  archive: true
parallelism:
  jobs: 2
  pages: auto
verbosity: info
```

## The two sections

`parallelism` and `out_flags` are **maps with a closed set of sub-keys** — two settings of one
thing, grouped so the file reads as it thinks. Both behave the same way:

- a sub-key left out stays **unset**, so another source still decides it. A file naming only
  `archive` leaves `separate_out` alone;
- an **unknown sub-key is an error naming its full path** — `parallelism.pipelines`,
  `out_flags.compressed`. A level or a flag that was considered and never implemented must not look
  active because a file mentioned it;
- a section that is not a map at all is an error.

Note that `out_flags.archive` is spelled as the flag is, not as the field is: `compressed` is the
internal name and is rejected here.

## Types

The file wants **real YAML types**, unlike the environment, which only has strings: `save_pdf: true`
is a boolean, not the string `"true"`. Parallelism is the one place both are accepted, because YAML
already distinguishes `2` from `auto` and refusing a quoted number would be a subtlety with no gain.

## Unknown keys are errors

At the top level as in the two sections. A misspelled key that is silently ignored configures
nothing and says nothing about it, and the user is left believing a setting took effect. A file with
a single unknown key fails the run, naming the file and the key.

An empty file, or one holding nothing but `null`, contributes nothing and is not an error. A
top-level document that is not a mapping is.

## Where the file is looked for

Unless `--config` names one, three tiers are searched in **decreasing** precedence, and the first
file found wins outright — the tiers do not merge with each other.

1. **The working directory** — a file named, case-insensitively, `config-freeports.yaml` or
   `freeports-config.yaml`, in their usual punctuation and extension variants: a leading dot,
   `conf` for `config`, `-`, `_` or `.` as separator, `.yml` for `.yaml`. This is the per-project
   configuration, and the one to commit next to a batch file.
2. **The user tier** — the operating system's *local* configuration directory, holding
   `freeports.yaml` or `freeports.yml`. Local rather than roaming, deliberately: a file naming
   machine-local paths should not follow you to another machine and point at directories that do not
   exist there.
3. **The system tier** — the XDG configuration directories and then `/etc` on POSIX; the
   program-data directory and then the system root on Windows.

No file anywhere is **not an error**. It simply means this layer contributes nothing.

Both platform branches of the system tier are always compiled, whatever the machine — which is how
the tests exercise both regardless of which system runs them.
