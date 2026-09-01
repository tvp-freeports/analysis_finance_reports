# The two tooling commands

`freeports` extracts. The two other distributions do the jobs around it: `freeports-dev` is what a
format author works in all day, and `freeports-validate` is how a claim about a file gets a name and
a signature attached. Neither is needed to run an extraction; both are needed to maintain a formats
repository.

| Command | Answers | Needs |
|---|---|---|
| `freeports-dev` | *what does the engine see on this page, and does it still see it tomorrow?* | Python, the engine |
| `freeports-validate` | *who vouched for this file, under which published methodology, and is that still true?* | GnuPG and `jq`, plus two Python packages it installs itself |

Both are installed from the source tree ({doc}`../usage/installation`):

```console
$ pip install packages/freeports_dev packages/freeports_validate
```

## Both commands need to find a formats repository

Every subcommand of either tool works *inside* a formats repository, and each resolves which one in
the same three steps, in this order:

1. an explicit `--repo` / `-r` argument;
2. the `FREEPORTS_FORMATS_REPO` environment variable;
3. the current working directory.

```{warning}
`FREEPORTS_FORMATS_REPO` is **not** the same variable as the engine's
`FREEPORTS_FORMATS_REPO_PATH` ({doc}`../usage/configuration/index`). Setting only one of the two leaves the other
tool falling back to the working directory, which is the kind of failure that looks like a bug in
whichever command you happened to run second. Export both, to the same path:

    export FREEPORTS_FORMATS_REPO=~/work/my-formats
    export FREEPORTS_FORMATS_REPO_PATH=~/work/my-formats
```

`freeports-dev` additionally checks that the directory really is one — `metadata/formats.csv` has to
exist — and refuses with a message naming the path rather than failing later and obscurely.

---

## `freeports-dev`

### The subcommands

| Subcommand | Does |
|---|---|
| `init-format-repo <path>` | writes an empty formats repository at `<path>` |
| `setup-input-db` | writes `tests/input_db/` with a minimal `TEST` company list |
| `inspect-document` | classifies the pages of a report: which page is what |
| `inspect-page` | shows what one page looks like at a chosen stage of the pipeline |
| `make-tests` | freezes one page's current behaviour as JSON fixtures |
| `test` | runs the repository's tests through pytest |

They are meant to be used in that order the first time and in the
`inspect-document → inspect-page → make-tests → test` loop after that; {doc}`writing-a-format`
walks through the loop with a real format, and this page is the reference for the options.

### Starting a repository

```console
$ freeports-dev init-format-repo ~/work/my-formats
$ cd ~/work/my-formats
$ freeports-dev setup-input-db
```

`init-format-repo` writes `package.yaml`, the `metadata/` tables, the `content/` tree and an empty
`tests/`, then validates the generated `package.yaml` against its JSON schema and says so. It is a
skeleton, not a working repository: it supports no format until you add one. See
{doc}`repository` for what each generated file is.

`setup-input-db` copies a minimal input database into `tests/input_db/`, with a single list named
`TEST`. It exists so that the tests of a repository do not depend on a database maintained
elsewhere — a format's test must fail because the *format* broke, never because someone edited a
company list. For real runs you want a real database; see {doc}`../input-db`.

### `inspect-document` — which page is what

```console
$ freeports-dev inspect-document --format CARNE-EN23
Page 1: unclassified
Page 2: unclassified
Page 25: investments
```

| Option | Meaning |
|---|---|
| `--format` / `-f` | the format to load. **Required** |
| `--page` / `-p` | classify only this page (1-based); omitted, every page is classified |
| `--report` | the PDF. Defaults to `tests/formats/<FORMAT>/report.pdf` |
| `--repo` / `-r` | the formats repository |

Run this before anything else. A page that should be `investments` and comes back `unclassified` is
a classification problem, and nothing downstream of it can be right until it is fixed — chasing the
extraction of a page the engine never selected is the single most common way to lose an afternoon.

### `inspect-page` — what the engine sees, stage by stage

```console
$ freeports-dev inspect-page --format CARNE-EN23 --page 25 --page-type investments
```

| Option | Meaning |
|---|---|
| `--format` / `-f` | the format. **Required** |
| `--page` / `-p` | the page number, 1-based. **Required** |
| `--page-type` / `-t` | the page class to process it as. Default `investments` |
| `--mode` / `-m` | what to print — see below. Default `results` |
| `--strings` | the strings to look for, **required** by the three line-set modes |
| `--report` | the PDF. Defaults to `tests/formats/<FORMAT>/report.pdf` |
| `--filter-data` | a `.pkl` of filter data. Defaults to the repository's test companies |
| `--repo` / `-r` | the formats repository |

The modes fall into three families, and choosing the right family is most of the skill:

| `--mode` | Prints | Use it to |
|---|---|---|
| `pdf_blks` | the blocks after `pdf_extract` | see whether the *layout* predicate found anything |
| `txt_blks` | the blocks after `text_filter` | see whether the *meaning* was read out of them |
| `results` | the deserialized rows | see the finished product of the page |
| `table_md`, `table_ascii` | the PDF blocks rendered as a table | read a tabular page as a table, with its row and column metadata |
| `structured`, `semistructured`, `unstructured` | which lines match `--strings`, as that level's selection sees them | learn what font, size and band a value is actually set in |

The first three are the diagnosis: running them in order tells you *which of the three segments* is
wrong, which is a much smaller question than "the page is wrong". The line-set modes answer the
question that comes before writing a selection at all:

```console
$ freeports-dev inspect-page -f CARNE-EN23 -p 25 -m structured --strings "Total Assets"
```

`--page-type` matters even in the pipeline modes: it is what decides which pipeline the page is fed
through, so inspecting a page as the wrong class shows you a correct answer to the wrong question.

### `make-tests` — freeze a page

```console
$ freeports-dev make-tests --format CARNE-EN23 --page 25 --page-type investments
```

| Option | Meaning |
|---|---|
| `--format` / `-f`, `--page` / `-p`, `--page-type` / `-t` | as above. All three **required** |
| `--document` / `-d` | the document variant, for a format with several |
| `--report`, `--filter-data`, `--repo` | as above |
| `--noconfirm` | do not ask before writing each fixture |
| `--skip-pdf-blks`, `--skip-txt-blks`, `--skip-results` | leave that fixture out |
| `--print_pdf_blks`, `--print_txt_blks` | also print those blocks while generating |
| `--noprint_results` | do not print the results while generating |

It writes the three per-page fixtures — the PDF blocks, the text blocks, the results — as JSON under
`tests/formats/<FORMAT>/pages/`, after showing you what it is about to record and asking. Read what
it prints before answering: `make-tests` records *what the code does now*, so confirming a wrong
result promotes a bug into the specification, and the test will then defend it.

`--noconfirm` is for regenerating fixtures you have already reviewed, not for generating new ones.

### `test` — run the repository's tests

```console
$ freeports-dev test                         # everything
$ freeports-dev test --format CARNE-EN23     # one format
$ freeports-dev test -- -x -k investments    # anything after `--` goes to pytest
```

It is pytest, with the `freeports_dev` plugin doing the collection: a directory named like a format
in `metadata/formats.csv` becomes a test node, and the per-page fixtures and the `out/` reference
become the tests under it. `--rootdir` is set to the repository, so the exit status is pytest's own
and a CI job needs nothing else.

Two kinds of test live there and they cost very different amounts. The per-page tests are fast, and
they are what you run every few minutes while working. The whole-document test replays the entire
report and compares the output tables against `tests/formats/<FORMAT>/out/`; it is slow, it is
marked `integration_tests`, and it is the one that actually says the format works.

```{important}
`tests/formats/<FORMAT>/out/**` is the repository's specification, not a snapshot. When a run
diverges from it, the assumption is that the engine changed, not that the expectation was wrong.
Regenerating one of those files is a deliberate act with a reason written down — never a way to turn
a red test green — and it is also what the grants of the next section are *about*.
```

---

## `freeports-validate`

Everything this command does revolves around one file: your **validation document**, a YAML file at
`<repo>/validation/<your_name>.yaml`, signed by your GPG key. {doc}`../validation` explains why the
mechanism is shaped this way; this section is how to operate it.

### Prerequisites

Unlike `freeports-dev`, this tool is a set of shell scripts, so it depends on **programs**. Two of
them are Python packages and are installed with it; the rest must come from your system:

| Program | Used for | Comes from |
|---|---|---|
| `yq` | every read and write of the YAML document | installed with `freeports-validate` |
| `check-jsonschema` | validating the document against its schema | installed with `freeports-validate` |
| `jq` | `yq` shells out to it — it *is* the expression engine | your system's package manager |
| `gpg` | signing and verifying, and as the source of your identity | GnuPG 2 |
| `sha256sum`, `realpath` | content hashes and repository-relative paths | GNU coreutils |

```{warning}
**Two unrelated programs are called `yq`, and this project needs a specific one.**

| | |
|---|---|
| **`yq` (kislyuk), on PyPI** | a thin wrapper around **jq**: it converts YAML to JSON, runs a **jq** filter, converts back. **This is the one required.** |
| `yq` (mikefarah), a Go binary | an independent implementation with its own expression language (`sortKeys`, `strenv`) |

They are not forks of one another; they collided on a name. Distributions disagree about which one
`yq` means, so if the signing commands fail with messages about undefined functions, check what you
have:

    $ yq --version
    yq 4.1.2
    jq-1.8.2          ← a jq version underneath means you have the right one

Installing `freeports-validate` pulls in the right one. It is also why the filters in these scripts
are plain jq: the signature is computed over `yq -y -S 'del(.sign)'`, and `-S` is **jq's own**
recursive key sort — an implementation detail that has to be identical for everyone, or one person's
signature cannot be verified by another.
```

### The identity: your GPG key

The tool has no user accounts. **Your key is your identity**, and the name and email written into
your validation document are read out of the key's user ID — not typed in, and therefore not
something you can get wrong in one place and right in another.

**1. Create a key**, if you do not already have one you want to use for this:

```console
$ gpg --full-generate-key
```

Choose the defaults for the algorithm, give it an expiry you are willing to maintain, and enter a
real name and a real email at the user-ID prompt. Both are required: the scripts split the UID on
`<…>`, so a key with no email produces a document that fails schema validation, and a key with no
name produces a document with nowhere to live.

**2. Take the key's fingerprint** — the full 40 hex digits, not the short or long key ID:

```console
$ gpg --list-secret-keys --with-colons --fingerprint | awk -F: '/^fpr:/ {print $10; exit}'
E61BCDC8F81AD6CB553ED5801E7C5644FDF4E304
```

```{warning}
This is the detail that catches everyone. The schema requires `who.pubkey_id` to be exactly
**40 uppercase hex digits** — a fingerprint. `gpg --list-keys --keyid-format=long`, the incantation
most documentation reaches for, prints the 16-digit long ID, and a document built from it is
rejected by `check-jsonschema` with a message about a pattern rather than about the key.
```

**3. Tell the tool which key to use**, through `AFINANCE_VALIDATION_KEYID`. Every subcommand refuses
to start without it:

```console
$ export AFINANCE_VALIDATION_KEYID=E61BCDC8F81AD6CB553ED5801E7C5644FDF4E304
```

`packages/freeports_validate/.env.template` is that one line, ready to copy next to your other
per-project environment settings.

**4. Publish the public half**, if anyone else will ever verify your grants. Verification is
`gpg --verify` against the signer's public key: without it, `check-grants` can confirm that a
document is well formed and that its hashes are current, but not that the signature is yours.

```console
$ gpg --armor --export E61BCDC8F81AD6CB553ED5801E7C5644FDF4E304 > oreste.pub.asc
```

Symmetrically, to check someone else's grants you need *their* public key in your keyring —
`gpg --import their.pub.asc`. A repository with several contributors is a repository where each of
them has imported the others.

### Creating your validation document

```console
$ freeports-validate create-document
```

It fills the template from your key — name, email, fingerprint — stamps the `version` field with the
hash of the general methodology page as shipped by the installed `freeports-validate`, validates the
result against the schema, and writes it to `<repo>/validation/<your_name>.yaml`, the name lowercased
with spaces turned into underscores. It refuses to overwrite an existing document, and the new one
is unsigned:

```yaml
version: d7f23dc8ad3014b4f9b7a0e550f24c83531559c46e91d28d25706c323b890323
who:
  name: Oreste Sciacqualegni
  email: oreste@example.org
  pubkey_id: E61BCDC8F81AD6CB553ED5801E7C5644FDF4E304
methodologies: []
data: []
sign: ~
```

Sign it before doing anything else — every other subcommand verifies the existing signature *before*
it will modify the document, and an unsigned document is refused:

```console
$ freeports-validate sign-document
```

### Adopting a methodology, then granting files

A grant is always *a file, under a methodology*, so the methodology has to be adopted first. Three
are published today — `basic check`, `golden standard`, `agreement and good faith` — and
{doc}`../validation` says what each one claims. Names are normalised: underscores become spaces and
everything is lowercased, so `golden_standard`, `Golden Standard` and `golden standard` are one
methodology.

```console
$ freeports-validate grant with "basic check"                      # adopt it
$ freeports-validate grant tests/formats/CARNE-EN23/out/*.csv \
                           with "basic check"                       # vouch for files
```

Adopting a methodology records the hash of its page; granting a file records the file's own hash and
its path relative to the repository root. Both re-sign the document immediately, so it is never left
in a state where its contents and its signature disagree.

To withdraw:

```console
$ freeports-validate ungrant <files…> with "basic check"   # those files, that methodology
$ freeports-validate ungrant <files…> with any             # those files, every methodology
$ freeports-validate ungrant with "basic check"            # the methodology and everything under it
```

The last form asks for confirmation, because it removes every grant made under that methodology at
once.

### Checking

```console
$ freeports-validate check-grants                    # your own document
$ freeports-validate check-grants someone_else.yaml  # another contributor's
```

It reports, item by item: the schema, the signature, the general-methodology version, the hash of
every adopted methodology, and the hash of every granted file. The exit status is non-zero if any of
them fails, which is what makes it a CI step rather than a report you read.

```{warning}
With no argument it checks **only your own** document — the one belonging to
`AFINANCE_VALIDATION_KEYID`. If you have none in this repository it says so and then reports *"All
validation documents passed verification"*, having verified nothing. In continuous integration,
name the documents to check, or iterate over `validation/*.yaml`; a green bare `check-grants` is not
evidence that a repository's grants hold.
```

A failing signature on a document that is not yours usually means its author's public key is not in
your keyring, not that the document was tampered with. Import it before concluding anything:
`gpg --import their.pub.asc`.

It answers one narrow question — *are these grants still about these files* — and it is not a
substitute for the tests passing, any more than the tests are a substitute for it. One says the code
still does what it did yesterday; the other says a person put their name to the result.

### Asking who stands behind what

| Command | Lists | Grouped by | Alternative |
|---|---|---|---|
| `who-grants <files…>` | who vouched for these files | contributor | `-m`, by methodology |
| `granted-by <contributor>` | what this person vouched for | methodology | `-f`, by file |
| `granted-with <methodology>` | what was vouched for under this methodology | file | `-c`, by contributor |

A contributor can be named by their name, their email or their key fingerprint — whichever you have.

### After a file legitimately changes

A grant is a claim about *bytes*. When granted content changes for a good reason — a format was
fixed, and its reference output genuinely should differ — the grant does not follow it
automatically, and that is the design working. Restating it is explicit:

```console
$ freeports-validate update file <path> with "basic check"   # one file's hash
$ freeports-validate update methodology "basic check"        # a methodology page changed
$ freeports-validate update version                          # the general methodology changed
```

Each re-signs afterwards. `update methodology` is the heavy one: a changed methodology page means
the claims made under it were made about a text that no longer exists, so it **drops every file
granted under that methodology**, after asking. That is deliberate — the alternative would be
carrying old claims forward under a new meaning.

```{caution}
`update` is not the way to silence a failing `check-grants`. It re-states an intention to vouch, and
it should be run by the person who has actually confirmed that the change was expected. Running it
because a check went red converts a real signal into a signature.
```

### Where everything lives

| Path | What |
|---|---|
| `<repo>/validation/<name>.yaml` | one validation document per contributor |
| `<repo>/package.yaml` → `info.validation_sha256` | ties the repository to the document vouching for it |
| the installed package's `docs/validation/` | the methodology pages, whose hashes the documents pin |
| the installed package's `lib/document.schema.json` | the schema every document is checked against |

The methodology pages ship **with the tool**, not with the repository, and that is why upgrading
`freeports-validate` can invalidate documents: the text a grant refers to is the text the installed
version carries. The same pages are reproduced in {doc}`the validation section <../../validation/index>`
of this site, and — for the same reason — cannot be edited as ordinary prose.
