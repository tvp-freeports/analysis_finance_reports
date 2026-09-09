# The command-line tools

`freeports` extracts. The two other distributions do the jobs around it: `freeports-dev` is what a
format author works in all day, and `freeports-validate` is how a claim about a file gets a name and
a signature attached. Neither is needed to run an extraction; both are needed to maintain a formats
repository.

| Command | Answers | Needs |
|---|---|---|
| `freeports-dev` | *what does the engine see on this page, and does it still see it tomorrow?* | Python, the engine |
| `freeports-validate` | *who vouched for this file, under which published methodology, and is that still true?* | GnuPG, `jq` and `curl`, plus two Python packages it installs itself. Not the engine, unless you want the configuration file |

Both are installed from the source tree ({doc}`../../start/install`):

```console
$ pip install packages/freeports_dev packages/freeports_validate
```

```{toctree}
:maxdepth: 1
:hidden:

freeports
freeports-dev
freeports-validate
methodology-sources
coverage-report
```

## What each page here covers
| Page | Is |
|---|---|
| {doc}`freeports` | every option of the extraction command, with its default and its validation |
| {doc}`freeports-dev` | the format author's loop: inspect, freeze, test, and creating a repository |
| {doc}`freeports-validate` | keys, validation documents, granting, and checking what a repository claims |
| {doc}`methodology-sources` | how a methodology name is resolved into a body of text |
| {doc}`coverage-report` | `freeports-validate report`, and the shapes it renders |

These pages say *what each option does*. They do not say which of them you want — that is the job of
whichever guide you came from: {doc}`../../guides/user/index` for running the engine,
{doc}`../../guides/formats/index` for writing a format, {doc}`../../guides/grants/index` for
vouching for one.

## Both commands need to find a formats repository

Every subcommand of either tool works *inside* a formats repository, and both resolve which one the
way the engine resolves everything: **command line, then environment, then configuration file, then a
default**.

1. `--repo` / `-r` / `--formats-directory` / `-F` — the engine's own four spellings, all accepted;
2. `FREEPORTS_FORMATS_REPO_PATH` — again the engine's variable, not a second one;
3. `formats_repo` in the configuration file;
4. the default: the working directory, except for `freeports-validate`, which first walks up to the
   enclosing Git repository.

That is one path, written once, found by all three commands. {doc}`../configuration/dev-and-validate` is the full
account — the two optional sections, the environment prefixes, the precedence.

`freeports-dev` additionally checks that the directory really is one — `metadata/formats.csv` has to
exist — and refuses with a message naming the path rather than failing later and obscurely.

```{note}
`FREEPORTS_FORMATS_REPO`, without the `_PATH`, used to be a **second** variable meaning the same
thing, so setting one left the other command falling back to the working directory. The two are now
one, and the old name is **no longer read at all** — a profile still exporting it silently
configures nothing.
```

