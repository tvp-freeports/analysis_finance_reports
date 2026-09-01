# Configuring the two tooling commands

`freeports-dev` and `freeports-validate` are configured the same way `freeports` is, and in most
cases *by the same file*. This page is the whole of it: what each command reads, where from, and
which name a setting has in each of the three places.

If you only extract data, none of this concerns you — see {doc}`../usage/configuration/index`
instead. The two sections described here are **optional**, and a configuration file that never
mentions them is not incomplete.

## One setting, three spellings

Every setting exists in three places, and the three names differ only in punctuation:

| Where | Shared with the engine | `freeports-dev` | `freeports-validate` |
|---|---|---|---|
| configuration file | a **top-level** key | under **`dev:`** | under **`validate:`** |
| environment | `FREEPORTS_…` | `FREEPORTS_DEV_…` | `FREEPORTS_VALIDATE_…` |
| command line | the engine's flag | the same flag | the same flag |

So the page type is `dev.page_type`, `FREEPORTS_DEV_PAGE_TYPE` and `--page-type`, and knowing one
tells you the other two. The prefix says *which command owns the setting*, which is also the answer
to "may I delete this from my configuration file" — anything under `dev:` or `validate:` only ever
matters to a format author.

## Precedence among the three sources
**Command line, then environment, then configuration file, then the command's own default.** The
same order the engine uses, minus the tiers that have no meaning here — there is no batch row.

The merge is **per setting, not per source**, exactly as in the engine: naming the repository on the
command line does not discard the target lists the file declares.

```console
$ freeports-dev test --repo ~/other-repo     # repository from here, everything else from the file
```

## What is shared, and why it is not in a section

The formats repository and the input database are **top-level keys**, the same ones the engine
reads. That is deliberate, and it is the point of the whole arrangement: a format author writes the
path once and all three commands find it.

```yaml
formats_repo: ~/work/my-formats
db_path: ~/work/input-db
```

A `tools:` section holding a second copy of the repository path would have been the obvious design
and the wrong one — two places to write the same thing is two places to write it differently.

## The `dev` section

```yaml
dev:
  target_lists: [TEST]
  noconfirm: false
  page_type: investments
```

| Key | Environment | Flag | Default | Means |
|---|---|---|---|---|
| `dev.target_lists` | `FREEPORTS_DEV_TARGET_LIST` | `--target-list` / `-T` | `[TEST]` | the lists a repository's tests search for |
| `dev.noconfirm` | `FREEPORTS_DEV_NOCONFIRM` | `--noconfirm` | `false` | skip the `make-tests` prompts |
| `dev.page_type` | `FREEPORTS_DEV_PAGE_TYPE` | `--page-type` / `-t` | `investments` | the page class `make-tests` and `inspect-page` assume |

As in the engine, **an absent flag is unset, not false**: `--noconfirm` can only ever switch the
setting on, so a command line that does not mention it leaves alone a file that did. To switch it
back off, use a source that can spell false — `FREEPORTS_DEV_NOCONFIRM=false`, or `noconfirm: false`.

And as in the engine, `FREEPORTS_DEV_TARGET_LIST` is **one** list, the whole raw value, never split:
a list whose name contains a comma still names one list. Several lists need a source with a list
type — the flag, or the YAML file.

```{caution}
`dev.target_lists` decides what a repository's tests look for, and therefore what its reference
output contains. Setting it in a **user- or system-tier** file changes the behaviour of every
formats repository on the machine, including ones built against `TEST`. It belongs in the
repository's own `freeports-config.yaml`, next to the tests it governs.
```

## The `validate` section

```yaml
validate:
  key_id: E61BCDC8F81AD6CB553ED5801E7C5644FDF4E304
```

| Key | Environment | Flag | Default | Means |
|---|---|---|---|---|
| `validate.key_id` | `FREEPORTS_VALIDATE_KEY_ID` | `--key-id` / `-k` | none — required | the GPG key that is your identity |

One key, and it is the one worth writing down: it is required by every subcommand, it never changes,
and it is the difference between the tool knowing who you are and refusing to start.

## Finding the repository

Both commands accept the engine's own spellings — `--repo`, `-r`, `--formats-directory`, `-F` — and
read `FREEPORTS_FORMATS_REPO_PATH` and the file's `formats_repo`. Where they differ is the last
resort, and the difference is intentional:

| | Falls back to |
|---|---|
| `freeports`, `freeports-dev` | the working directory |
| `freeports-validate` | the enclosing Git repository, then the working directory |

`freeports-validate` walks up because a validation document lives at the repository *root* while
grants are issued from wherever in the tree the granted file happens to be — `freeports-validate
grant tests/formats/CARNE-EN23/out/investments.csv with "basic check"` should work from inside
`tests/`. `freeports-dev` has no such need: it checks that `metadata/formats.csv` exists and says so
plainly when it does not.

## Reading the file needs the engine — for one of the two

`freeports-dev` requires `freeports` anyway, so it always reads the file.

`freeports-validate` does not. Verifying somebody else's grants should not mean installing a PDF
extractor, so the engine is an **optional** dependency there:

```console
$ pip install 'freeports-validate[config]'    # with the configuration-file tier
$ pip install freeports-validate              # command line and environment only
```

Without it, the command works exactly as before and simply has no file tier; `--config` then reports
that it needs the engine rather than silently ignoring the file. There is deliberately **no** second
implementation of the search for a configuration file: the tiers, the accepted filenames and the
refusal of unknown keys are the engine's answer, and there is one of them.

## A complete example for a formats repository
One file, at the root of a formats repository, configuring all three commands:

```yaml
# freeports-config.yaml
formats_repo: .
db_path: ~/work/input-db
target_lists:
  - controversial_weapons
out_path: ~/work/results

dev:
  target_lists: [TEST]

validate:
  key_id: E61BCDC8F81AD6CB553ED5801E7C5644FDF4E304
```

Note the two `target_lists`. The top-level one is what a **real extraction** searches for; `dev`'s is
what the **repository's own tests** search for. They are different questions, and a repository that
conflated them would have tests whose result depended on the user's day job.

## Unknown keys are errors here too

`dev.tagret_lists` fails, naming the full path of the key. The engine parses these sections even
though it never reads them, precisely so that this rule reaches inside them: a misspelled setting
that is silently ignored configures nothing and reports nothing.
