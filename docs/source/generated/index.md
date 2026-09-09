# Generated pages

Nothing in this section is written by a person. Each page is the output of a command, and each one
names that command, because a figure you cannot re-derive is a figure you are being asked to take on
faith.

```{toctree}
:maxdepth: 1
:hidden:

python-api
rust-api
../validation/report/index
../dev/ci-report/index
```

## What is here
| Page | Written by | Says |
|---|---|---|
| {doc}`python-api` | `sphinx-autosummary`, from the installed packages | the API of `freeports`, `freeports_dev` and `freeports_validate` |
| {doc}`rust-api` | `cargo doc` | the crate's own API, module by module |
| {doc}`../validation/report/index` | `freeports-validate report` | who has vouched for which files, under which methodology |
| {doc}`../dev/ci-report/index` | `freeports-dev ci-report` | what the last gate run measured, against what minimum |

## How to check any of it
```console
$ freeports-validate check-grants     # the grants, verified against your own keyring
$ freeports-dev ci-check              # the metrics, and whether they are current
$ make docs                           # the two API references, rebuilt from the source
```

The two report pages are **not** rebuilt when the site is built, and that is not an omission.
Whether a signature is valid is computed from the keyring of the machine doing the computing, and a
documentation builder has no public keys in it — a page generated there would announce that every
grant in the repository is invalid. The CI figures have the same problem for a different reason: the
measurements live in a directory that is not in version control, so a builder has no figures at all.
Both are therefore written where the keys and the measurements are — by the commit hook, on a
developer's machine — committed, and merely copied when the site is built.

Which is itself worth reading as a statement about the project: {doc}`../guides/institutional/strengths-and-limits`.
