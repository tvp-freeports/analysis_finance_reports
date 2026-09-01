# The command line

The strongest source except in batch mode, where a row overrides it. Per-option detail is in
{doc}`options`; this page is the mapping and the rules specific to this source.

## Flag to option

| Flag | Option | Note |
|---|---|---|
| `--input`, `--report`, `-i` | `reports` | repeatable, several values after one occurrence |
| `--target-list`, `-T` | `target_lists` | likewise |
| `--format`, `-f` | `format` | |
| `--formats-directory`, `--repo`, `-F`, `-r` | `formats_repo_path` | |
| `--db-directory`, `-I` | `input_db_path` | |
| `--out`, `-o` | `out_path` | |
| `--out-profile`, `-P` | `out_profile` | one of the three names, case-insensitively |
| `--separate-out` | `separate_out` | present = true |
| `--archive`, `-z` | `compressed` | present = true |
| `--no-download` | `save_pdf` | present = **false** |
| `--batch`, `-b` | `batch_file` | |
| `--config` | `config_file` | |
| `--workers`, `-j` | `n_workers` | positive integer or `auto` |
| `--jobs` | `parallelism_jobs` | long form only |
| `--pages` | `parallelism_pages` | |
| `-v`, `-q` | `verbosity` | counted; only when at least one is given |
| `--internal-worker` | — | hidden; the channel between two copies of the binary, not a setting |

## Rules of this source

**An absent flag leaves its option unset, never set to a default.** Defaults belong to the defaults
tier, and a command line that set everything would make every other source unreachable — a
configuration file could then never contribute anything.

The visible consequence is that the three boolean flags can only be switched **on** here.
`--archive` absent means *nothing said*, not false. To say false, use a source that can spell it:
`FREEPORTS_ARCHIVE=false`, or `archive: false`.

**`-v` and `-q` are independent dials**, not opposed flags. Both are counted, the net offset
`verbose - quiet` is added to the default and clamped, and using them together is never an error.
The option is left unset unless at least one of the two appears.

**Repeatable flags leave the option unset when absent**, not empty. `-i` given zero times is not
"no documents", it is "this source says nothing about documents".

## Validation at this source

Three things are rejected here rather than later, each with the flag's own name in the message so
that someone who mistyped `--pages` reads `--pages` and not a generic sentence about parallelism:

- a document specifier that does not parse ({doc}`../documents`);
- a parallelism value that is not a positive integer or `auto` — including zero and negatives, which
  is why hyphen values have to be accepted at all: without that, `-1` would be taken for an unknown
  option and the typed error could never be reached;
- an output profile that is not one of the three names.
