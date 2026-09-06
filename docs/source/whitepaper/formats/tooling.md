# The two tooling commands

`freeports` extracts. The two other distributions do the jobs around it: `freeports-dev` is what a
format author works in all day, and `freeports-validate` is how a claim about a file gets a name and
a signature attached. Neither is needed to run an extraction; both are needed to maintain a formats
repository.

| Command | Answers | Needs |
|---|---|---|
| `freeports-dev` | *what does the engine see on this page, and does it still see it tomorrow?* | Python, the engine |
| `freeports-validate` | *who vouched for this file, under which published methodology, and is that still true?* | GnuPG and `jq`, plus two Python packages it installs itself. Not the engine, unless you want the configuration file |

Both are installed from the source tree ({doc}`../usage/installation`):

```console
$ pip install packages/freeports_dev packages/freeports_validate
```

## Both commands need to find a formats repository

Every subcommand of either tool works *inside* a formats repository, and both resolve which one the
way the engine resolves everything: **command line, then environment, then configuration file, then a
default**.

1. `--repo` / `-r` / `--formats-directory` / `-F` — the engine's own four spellings, all accepted;
2. `FREEPORTS_FORMATS_REPO_PATH` — again the engine's variable, not a second one;
3. `formats_repo` in the configuration file;
4. the default: the working directory, except for `freeports-validate`, which first walks up to the
   enclosing Git repository.

That is one path, written once, found by all three commands. {doc}`configuration` is the full
account — the two optional sections, the environment prefixes, the precedence.

`freeports-dev` additionally checks that the directory really is one — `metadata/formats.csv` has to
exist — and refuses with a message naming the path rather than failing later and obscurely.

```{note}
`FREEPORTS_FORMATS_REPO`, without the `_PATH`, used to be a **second** variable meaning the same
thing, so setting one left the other command falling back to the working directory. The two are now
one, and the old name is **no longer read at all** — a profile still exporting it silently
configures nothing.
```

---

## `freeports-dev`

### The `freeports-dev` subcommands
| Subcommand | Does |
|---|---|
| `init-format-repo <path>` | writes an empty formats repository at `<path>` |
| `setup-input-db` | writes `tests/input_db/` with a minimal `TEST` company list |
| `init-input-db <path>` | writes an empty input database at `<path>`, to maintain as its own repository |
| `inspect-document` | classifies the pages of a report: which page is what |
| `inspect-page` | shows what one page looks like at a chosen stage of the pipeline |
| `make-tests` | freezes one page's current behaviour as JSON fixtures |
| `test` | runs the repository's tests through pytest |

They are meant to be used in that order the first time and in the
`inspect-document → inspect-page → make-tests → test` loop after that; {doc}`writing-a-format`
walks through the loop with a real format, and this page is the reference for the options.

### Creating an empty repository
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

`init-input-db` is the other half of that sentence and is not part of this loop: it starts a
database you intend to maintain, at a path of your choosing, as its own repository rather than
inside a formats repository. `--sample` fills its tables with the same example data
`setup-input-db` copies, as something to edit down. See {doc}`../input-db`.

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
| `--page` / `-p` | report only this page (1-based); omitted, every page is reported |
| `--report` | the PDF. Defaults to `tests/formats/<FORMAT>/report.pdf` |
| `--repo` / `-r`, `--config`, `--db-directory` / `-I` | the shared options; see {doc}`configuration` |

Run this before anything else. A page that should be `investments` and comes back `unclassified` is
a classification problem, and nothing downstream of it can be right until it is fixed — chasing the
extraction of a page the engine never selected is the single most common way to lose an afternoon.

`--page` narrows what is **printed**, not what is classified: the whole document is classified either
way. A format may supply a finalizer that rewrites the raw per-page answers looking at all of them
together — *every page after the holdings header is holdings* is the usual shape — so a page
classified on its own can get a different answer from the same page classified in its document, and
the isolated one is the wrong one.

### `inspect-page` — what the engine sees, stage by stage

```console
$ freeports-dev inspect-page --format CARNE-EN23 --page 25 --page-type investments
```

| Option | Meaning |
|---|---|
| `--format` / `-f` | the format. **Required** |
| `--page` / `-p` | the page number, 1-based. **Required** |
| `--page-type` / `-t` | the page class to process it as. Default `investments`, or `dev.page_type` |
| `--mode` / `-m` | what to print — see below. Default `results` |
| `--strings` | the strings to look for, **required** by the three line-set modes |
| `--report` | the PDF. Defaults to `tests/formats/<FORMAT>/report.pdf` |
| `--filter-data` | a `.pkl` of filter data. Defaults to the repository's test companies |
| `--target-list` / `-T` | which lists those test companies come from. Default `TEST` |
| `--repo` / `-r`, `--config`, `--db-directory` / `-I` | the shared options; see {doc}`configuration` |

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
| `--format` / `-f`, `--page` / `-p` | as above. Both **required** |
| `--page-type` / `-t` | as above. No longer required: `dev.page_type` can supply it, and it defaults to `investments` |
| `--document` / `-d` | the document variant, for a format with several |
| `--report`, `--filter-data`, `--target-list`, `--repo`, `--config`, `--db-directory` | as above |
| `--noconfirm` | do not ask before writing each fixture. Also `dev.noconfirm` |
| `--skip-pdf-blks`, `--skip-txt-blks`, `--skip-results` | leave that fixture out |
| `--print_pdf_blks`, `--print_txt_blks` | also print those blocks while generating |
| `--noprint_results` | do not print the results while generating |

It writes the three per-page fixtures — the PDF blocks, the text blocks, the results — as JSON under
`tests/formats/<FORMAT>/pages/`, after showing you what it is about to record and asking. Read what
it prints before answering: `make-tests` records *what the code does now*, so confirming a wrong
result promotes a bug into the specification, and the test will then defend it.

`--noconfirm` is for regenerating fixtures you have already reviewed, not for generating new ones.
As with the engine's boolean flags, the flag can only switch it **on**; to make it a standing choice
set `dev.noconfirm` in the configuration file, and to switch that back off write `false` there —
an absent flag says nothing, it does not say no.

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

### What `freeports-validate` needs installed
Unlike `freeports-dev`, this tool is a set of shell scripts, so it depends on **programs**. Two of
them are Python packages and are installed with it; the rest must come from your system:

| Program | Used for | Comes from |
|---|---|---|
| `yq` | every read and write of the YAML document | installed with `freeports-validate` |
| `check-jsonschema` | validating the document against its schema | installed with `freeports-validate` |
| `jq` | `yq` shells out to it — it *is* the expression engine | your system's package manager |
| `gpg` | signing and verifying, and as the source of your identity | GnuPG 2 |
| `sha256sum`, `realpath` | content hashes and repository-relative paths | GNU coreutils |
| `freeports` | **optional** — only to read settings from the configuration file | `pip install 'freeports-validate[config]'` |

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

### Prerequisite: the OpenPGP key

The tool has no user accounts. **Your key is your identity**, and the name and email written into
your validation document are read out of the key's user ID — not typed in, and therefore not
something you can get wrong in one place and right in another.

That is also the limit of what a key is for here. It says who is *speaking*, so it is asked for by
the subcommands that speak in your name, and by nothing else:

| Subcommand | Needs a key | Because |
|---|---|---|
| `create-document`, `grant`, `ungrant`, `update`, `sign-document` | **yes** | each writes, or signs, the one document that belongs to you |
| `check-grants` with no argument | optional | with a key it checks *your* document; without one there is nothing that is yours, so it checks **every** document in `validation/` |
| `check-grants <document>`, `who-grants`, `granted-by`, `granted-with` | **no** | they verify signed files against your keyring — a question about other people, not about you |

So reading what others have vouched for needs no key of your own. That is what lets `check-grants`
run in continuous integration, and lets anyone audit a repository they did not write.

```{note}
**`gpg` or `gpg2`?** The scripts call `gpg`, and what they need is **GnuPG 2**. Since GnuPG 2.2 the
`gpg` command *is* version 2 and `gpg2` survives as an alias for it, so on a current system
`gpg --version` reporting `2.x` is all you have to check. Where `gpg` is still GnuPG 1.4, install
GnuPG 2 and make `gpg` resolve to it — through your distribution's alternatives mechanism or a `PATH`
entry. Pointing the tool at `gpg2` instead is not an option: the name is written into the scripts.
```

**1. Create a key**, if you do not already have one you want to use for this:

```console
$ gpg --full-generate-key
```

Take the algorithm GnuPG offers — on 2.4 that is **Ed25519**, which is what this project's own
validation document is signed with. Give it an expiry you are willing to maintain; step 7 explains
why that is close to free. At the user-ID prompt enter a real name and a real email. Both are
required: the scripts split the UID on `<…>`, so a key with no email produces a document that fails
schema validation, and a key with no name produces a document with nowhere to live.

The same thing without prompts, for a container or a fresh machine:

```console
$ gpg --quick-generate-key "Ada Lovelace <ada@example.org>" default default 2y
```

**2. Keep the revocation certificate, and back up the secret key.** GnuPG writes a revocation
certificate the moment the key is created and prints where it put it:

```text
gpg: revocation certificate stored as '/home/ada/.gnupg/openpgp-revocs.d/<FINGERPRINT>.rev'
```

That file is the only way to announce that a key must no longer be trusted — and the moment you need
it is precisely the moment you can no longer produce one. Copy it somewhere that is not the machine
holding the key, and back up the secret half while you are there:

```console
$ gpg --armor --export-secret-keys <FINGERPRINT> > secret-backup.asc   # guard like a password
```

Losing the key does **not** invalidate grants you have already issued: verification needs only the
public half, which by then is in other people's keyrings. What it ends is your ability to issue or
amend any, under a name that is already in the repository.

**3. Take the key's fingerprint** — the full 40 hex digits, not the short or long key ID:

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

**4. Tell the tool which key to use.** The subcommands in the first row of the table above refuse to
start without it, and there are three ways to say it — the same three every other setting has
({doc}`configuration`):

```console
$ freeports-validate -k E61BCDC8F81AD6CB553ED5801E7C5644FDF4E304 grant <files…>   # this once
$ export FREEPORTS_VALIDATE_KEY_ID=E61BCDC8F81AD6CB553ED5801E7C5644FDF4E304       # this shell
```

```yaml
# freeports-config.yaml, next to the repository — the one worth writing down
validate:
  key_id: E61BCDC8F81AD6CB553ED5801E7C5644FDF4E304
```

`packages/freeports_validate/.env.template` is the environment form, ready to copy next to your
other per-project settings. Reading it from the configuration file needs the engine installed
alongside — `pip install 'freeports-validate[config]'` — because that is the one thing that knows
where a configuration file lives.

```{note}
This variable used to be called `AFINANCE_VALIDATION_KEYID`, a name from before the project was
called freeports and the only setting anywhere that did not begin with `FREEPORTS_`. The old name is
**no longer read**: a shell that exports only it gets the same refusal as one that sets nothing.
```

**5. Publish the public half**, because a signature nobody can check is a signature nobody reads.
Verification is `gpg --verify` against the signer's public key; without it, `check-grants` can
confirm that a document is well formed and that its hashes are current, but not that the signature
is yours.

```console
$ gpg --armor --export E61BCDC8F81AD6CB553ED5801E7C5644FDF4E304 > ada.pub.asc
```

Three places to put it, in increasing order of how little they ask of the person checking:

| Where | How | What it gives, and what it does not |
|---|---|---|
| the repository itself | commit `ada.pub.asc` beside your document | anyone with the checkout can verify, fetching nothing — but the key travels with the thing it authenticates, so on its own it attests to nothing |
| `keys.openpgp.org` | `gpg --keyserver hkps://keys.openpgp.org --send-keys <FINGERPRINT>`, then follow the confirmation mail | the server verifies the address before the key becomes searchable by email; unconfirmed, it is retrievable by fingerprint only |
| WKD, on a domain you control | publish under `.well-known/openpgpkey/` | `gpg --locate-keys ada@example.org` just finds it — the least ceremony for the person checking, and it ties the key to the domain |

Symmetrically, to check someone else's grants you need *their* public key in your keyring —
`gpg --import their.pub.asc`, or `gpg --locate-keys their@email`. A repository with several
contributors is a repository where each of them has imported the others.

**6. Certifying a key** — signing somebody else's — states that *you* have satisfied yourself that
the key belongs to the person named on it. Do it after confirming the fingerprint through some
channel other than the one that handed you the key:

```console
$ gpg --sign-key <THEIR FINGERPRINT>    # a certification others can see, once exported and published
$ gpg --lsign-key <THEIR FINGERPRINT>   # the same judgement, kept local to your keyring
```

```{warning}
**`freeports-validate` does not consult trust.** `gpg --verify` succeeds for a good signature from a
key you have never certified — it prints *"WARNING: This key is not certified with a trusted
signature"* and exits zero — and `check-grants` duly reports the signature as valid.

Certifying keys is therefore for **your** judgement about who a contributor is; it changes nothing
the tool decides. The mechanism ties a claim to a key. Tying that key to a person is yours to do.
```

What the verification step actually distinguishes, then, is narrower than it looks:

| Situation | `gpg --verify` | What `check-grants` reports |
|---|---|---|
| good signature, key certified or not | success | signature valid |
| good signature, signer's key has **expired** | success | signature valid |
| the document changed after it was signed | failure | invalid signature |
| the signer's public key is **not in your keyring** | failure | invalid signature |

The last row is the one that misleads, and it is worth knowing before you accuse anyone: a document
reported as having an invalid signature is far more often a key you are missing than a document
somebody tampered with.

**7. Expiry, renewal and revocation.** An expiry date is a dead-man's switch rather than a deadline —
it limits how long a key stands unattended, and extending it costs one command:

```console
$ gpg --quick-set-expire <FINGERPRINT> 2y
```

Then re-export and re-publish, since the new expiry is part of the public key. As the table above
records, **expiry does not retract anything**: signatures made while the key was valid keep
verifying afterwards, so letting a key lapse never silently invalidates grants you already issued.

Revocation is the other case, and it is a real statement rather than a lapse: this key must not be
trusted, because it was lost or compromised. Publish the certificate you saved in step 2 —

```console
$ gpg --import <FINGERPRINT>.rev
$ gpg --keyserver hkps://keys.openpgp.org --send-keys <FINGERPRINT>
```

— and then re-key: your document records the old fingerprint in `who.pubkey_id` and carries a
signature made with it. There is no subcommand for changing that, because it is not a routine
operation: replace `who.pubkey_id` with the new fingerprint by hand and run
`freeports-validate sign-document --update`. Grants signed by a compromised key should be
reconsidered, not merely re-signed — the point of the mechanism is that a name stands behind them.

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

### Checking grants
```console
$ freeports-validate check-grants                    # your own document
$ freeports-validate check-grants someone_else.yaml  # another contributor's
```

It reports, item by item: the schema, the signature, the general-methodology version, the hash of
every adopted methodology, and the hash of every granted file. The exit status is non-zero if any of
them fails, which is what makes it a CI step rather than a report you read.

With no argument, what it looks at depends on whether a key is configured — because that is what
decides whether any document here is *yours*:

| | Bare `check-grants` checks |
|---|---|
| a key is configured | **only your own** document, the one belonging to that key |
| no key is configured | **every** `validation/*.yaml` in the repository |

```{warning}
The first row has a trap in it. If you have a key configured but no document of your own in this
repository, `check-grants` says so and then reports *"All validation documents passed
verification"* — having verified nothing, because there was nothing of yours to verify. A green bare
`check-grants` from a contributor's own machine is not evidence that a repository's grants hold.

The second row is the form to use in continuous integration, and it needs no key precisely because
nothing there speaks in anyone's name: it verifies every document against the keyring it is given.
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
$ freeports-validate update file <path> with "basic check"   # one file's hash, if already granted
$ freeports-validate update methodology "basic check"        # a methodology page changed
$ freeports-validate update version                          # the general methodology changed
```

`update file` restates an existing claim; it does not create one. A file that was never granted
under that methodology is refused, naming the `grant` that would be the right command — it used to be
accepted, do nothing, and report success.

Each re-signs afterwards. `update methodology` is the heavy one: a changed methodology page means
the claims made under it were made about a text that no longer exists, so it **drops every file
granted under that methodology**, after asking. That is deliberate — the alternative would be
carrying old claims forward under a new meaning.

```{caution}
`update` is not the way to silence a failing `check-grants`. It re-states an intention to vouch, and
it should be run by the person who has actually confirmed that the change was expected. Running it
because a check went red converts a real signal into a signature.
```

### Where the validation files live
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
