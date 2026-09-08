# The two tooling commands

`freeports` extracts. The two other distributions do the jobs around it: `freeports-dev` is what a
format author works in all day, and `freeports-validate` is how a claim about a file gets a name and
a signature attached. Neither is needed to run an extraction; both are needed to maintain a formats
repository.

| Command | Answers | Needs |
|---|---|---|
| `freeports-dev` | *what does the engine see on this page, and does it still see it tomorrow?* | Python, the engine |
| `freeports-validate` | *who vouched for this file, under which published methodology, and is that still true?* | GnuPG, `jq` and `curl`, plus two Python packages it installs itself. Not the engine, unless you want the configuration file |

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

(what-init-format-repo-writes)=
#### The README, the report and the hook

It also writes everything a repository needs to *show* what it has been vouched for, because the
alternative is that each of those files gets invented separately by every person who ever makes a
formats repository:

| Path | What |
|---|---|
| `README.md` | the repository's front page: the three badges, the summary table between its markers, and links to the six pages below |
| `validation/report/<table>.md` | six pages, one per arrangement of the grants — named after the `--table` value that fills each one |
| `validation/report/badges/` | `grants-total`, `grants-coverage`, `check-grants`, each as an SVG and as shields.io endpoint JSON |
| `.githooks/pre-commit` | regenerates all nine before every commit |

All nine are then **filled straight away**, by running the command once — so a repository never
starts life with three broken images and six empty tables in it. That step is best-effort: if
`freeports-validate` is not installed, or the methodology pages cannot be resolved from this
machine, the initialisation says so, names the command to run later, and succeeds anyway. A skeleton
that could not be created without a reachable documentation server would be a worse tool than one
that leaves nine files to fill.

The hook runs after the test run, and **never refuses a commit over the report**. It collects once,
renders the nine artefacts, and `git add`s the ones it actually rewrote so the refresh is part of
the same commit as the change that caused it. Everything about it is conditional: no
`freeports-validate` on the path, no methodology it can resolve, or a page whose marker pair
somebody removed, and it says so on standard error and leaves the file alone. The figures depend on
a network, and a committer on a train still has to be able to commit.

To change where any of it goes, edit the hook: it is nine ordinary lines in a shell script the
repository owns, and there is deliberately no setting for it.

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
| `curl` | fetching a methodology page from the source it is published at | your system's package manager |
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
| `sources`, `check-methodology`, `collect`, `report` | **no** | they ask what your configuration resolves, what a page says, and what the repository has been vouched for. None of that is done in anybody's name |
| `refresh-links` | **no** | it rewrites a local page's pins. That page is not signed; the documents that pin *it* are, and they are somebody else's |

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

### Where methodology pages come from

A grant is a claim made **under a text**, so the tool has to be able to read that text. It no longer
carries one. Methodology pages are resolved from **sources** you configure, and the hash your
document records is the hash of whatever your sources gave you.

That is a deliberate trade, and it is worth stating both halves. The pages used to travel inside the
installed package, which meant upgrading `freeports-validate` silently changed what every grant in
every repository referred to. Now the text is a document at an address you chose — and the mirror
image of the old fragility is that whoever publishes that address can invalidate your grants by
editing the page. {doc}`../validation` says why that is the right way round.

**The source is not recorded in your document.** A validation document stores a methodology's *name*
and its *sha256*, and nothing about where either came from. The source is a contract between the
person who granted and the person who verifies, held in the configuration each of them writes. The
cost is that a hash mismatch is ambiguous — the page may have been rewritten, or the two of you may
simply be reading two publications of the same methodology — and `sources` below is where that cost
is paid back.

#### The setting

| Tier | Spelling | Type |
|---|---|---|
| configuration file | `validate.sources` | a YAML **list** of strings |
| environment | `FREEPORTS_VALIDATE_SOURCE` | **one** source — the whole value, never split |
| command line | `--source URI`, `-s URI` | repeatable |

With no tier naming anything, the default is the published documentation:
`https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt`.

Two rules about the list are worth learning once:

**The tiers do not merge.** The strongest tier that names any source provides the whole list, so a
command line with `-s` ignores the configuration file's list entirely. Merging would make *which
text did this hash come from* unanswerable from any one place. The case that wants two sources at
once — the published documentation **plus** your own methodologies — is served by listing both in
the same tier, which is why the file tier takes a list.

**The environment carries one source, and its value is never split.** Every separator one might pick
— comma, colon, space, semicolon — is a legal character in a URI or a path, so splitting would turn
one source you meant into two that do not exist. This is the same rule `FREEPORTS_TARGET_LIST`
follows, for the same reason.

**Order is priority.** A name is resolved from the first source that offers it; the rest are
shadowed, and `sources` says so.

#### The grammar of a source

A source is a pattern containing **exactly one `*`**. The `*` stands for the methodology's relative
name, which may itself contain a `/`; everything before it is the base and everything after it is
the extension.

```text
https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt
https://github.com/tvp-freeports/analysis_finance_reports/blob/main/docs/source/validation/*.rst
file:///home/me/my-methodologies/*.rst
../my-methodologies/*.rst
```

| What is being resolved | `*` becomes |
|---|---|
| the general methodology — your document's `version` field | `general_methodology` |
| a methodology named `basic check` | `methodologies/basic_check` |

| Rule | |
|---|---|
| zero, or more than one, `*` | a configuration error naming the offending source — it is not guessable which half is the base |
| a `github.com/<owner>/<repo>/blob/<ref>/<path>` URL | rewritten to `raw.githubusercontent.com/<owner>/<repo>/<ref>/<path>`, because fetching the `blob` page would hash a web page rather than a document, and would do it silently. Both spellings are correct input |
| `http://` | accepted, and warned about once per run: the hash is what compensates for an unauthenticated fetch, but you should know which kind you are doing |
| a bare relative path | accepted. In a **configuration file** it is resolved against the file's own directory, because that file is searched for and one line in it is read from many working directories; on the command line and in the environment it is relative to the shell you typed it in |

Sphinx publishes reStructuredText sources under `_sources/` with a `.txt` suffix appended, which is
why the default pattern ends in `.rst.txt` rather than `.rst`.

#### Fetching, the cache, and being offline

Pages are fetched with `curl -fsSL` and cached, content-addressed, under
`${XDG_CACHE_HOME:-$HOME/.cache}/freeports-validate/<sha256 of the URI>/<sha256 of the body>`. Every
distinct body ever seen for a URI is kept, and a `latest` symlink names the most recent one.

**Online, the cache is a fallback and never a shortcut.** A run with a network always fetches. A
cache consulted first would quietly pin a methodology to whatever was fetched the day you first saw
it — and the published text changing is precisely the event this whole mechanism exists to notice.

| Situation | What happens |
|---|---|
| the fetch succeeds | the fetched body is hashed, and cached |
| the fetch fails, no `--offline` | the check is **inconclusive** — not matching and not mismatched. `check-grants` exits `3` over it: nothing failed, and not everything was checked |
| the fetch fails, `--offline` given | the newest cached body is used, and every line that depended on it says so |
| `--offline`, nothing cached | reported as unreachable |

`--offline` is a setting like any other: `validate.offline`, `FREEPORTS_VALIDATE_OFFLINE`,
`--offline`. A machine can be permanently without egress — an air-gapped build, a runner with no
network — which is a property of the machine, written down once, rather than something to remember
on every run.

#### Seeing what your configuration resolves

`sources` is the answer to *why does my hash differ from yours*. Two people can compare its output
and see in one line which of the two possible causes it was. It needs no key.

```console
$ freeports-validate sources
ℹ️  Methodology sources, in priority order
==========================================
  1. https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt

A name is resolved from the first source that offers it.

ℹ️  What each name resolves to here
==========================================

the general methodology
  as:     general_methodology
  from:   https://docs.freeports.org/en/stable/_sources/validation/general_methodology.rst.txt
  sha256: d7f23dc8ad3014b4f9b7a0e550f24c83531559c46e91d28d25706c323b890323

basic check
  as:     methodologies/basic_check
  from:   https://docs.freeports.org/en/stable/_sources/validation/methodologies/basic_check.rst.txt
  sha256: eb4b396bf453fbf6273d7cca7ef0fc7fac925c03c20dd98bcae054d75a148f37
```

It reports the general methodology, every methodology adopted by a document in this repository, and
anything you name as an argument. It does **not** offer a catalogue: a remote source cannot be
enumerated at all — there is no way to ask a URL what else it serves — so a listing would silently
mean *the local ones* while looking like *all of them*. Where one source shadows another for the
same name, it says which, which is the fix for the failure where you edit a page, nothing changes,
and nothing anywhere says why.

### Creating your validation document

```console
$ freeports-validate create-document
```

It fills the template from your key — name, email, fingerprint — resolves the general methodology
from your sources and stamps the `version` field with its hash, validates the result against the
schema, and writes it to `<repo>/validation/<your_name>.yaml`, the name lowercased
with spaces turned into underscores — creating that directory when the repository has none, since
`init-format-repo` lays it down with the rest of the skeleton but a repository assembled by hand may
not have it. Creating it is announced. A *missing* `validation/` is also what a misresolved
repository root looks like, though, so the root has to look like a formats repository first — by the
`metadata/formats.csv` `freeports-dev` checks for — and one that does not is named and refused
rather than quietly populated. It refuses to overwrite an existing document, and the new one is
unsigned:

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

Adopting a methodology resolves its page and records the hash of that text; granting a file records
the file's own hash and its path relative to the repository root. Both re-sign the document
immediately, so it is never left in a state where its contents and its signature disagree.

Nothing is written until every file has passed. A grant is one act — the document is modified file
by file and signed once at the end — so refusing halfway would leave a document that had been
changed and not signed, which is the state `check-grants` reports as *no signature found*, reached
by a command doing what it was told.

To withdraw:

```console
$ freeports-validate ungrant <files…> with "basic check"   # those files, that methodology
$ freeports-validate ungrant <files…> with any             # those files, every methodology
$ freeports-validate ungrant with "basic check"            # the methodology and everything under it
```

The last form asks for confirmation, because it removes every grant made under that methodology at
once.

### What a methodology says it covers

A methodology page may declare which repository paths it applies to, in an ordinary visible section
titled `Supported paths`. Each entry is an inline literal with a prose gloss under it:

```rst
Supported paths
===============

``tests/formats/**/out/*.csv``
    The reference output of a format's test suite in a **formats repository**. A basic check here
    means the run that produced the file completed, the columns are the expected ones, and a human
    has looked at the values for obvious nonsense.
```

The gloss is the substance. `tests/formats/` means one thing in a formats repository and another in
the engine's own repository, and that ambiguity is resolved by an author writing down which they
mean — never by the tool guessing. So nothing here interprets the prose; everything here shows it.

The pattern grammar is deliberately tiny, because these patterns are read by people deciding whether
to trust a grant. A pattern that looked like a shell glob but was not one would be worse than no
pattern language at all:

| Token | Matches |
|---|---|
| `*` | any part of **one** path segment; never crosses a `/` |
| `**` | zero or more whole segments |
| a trailing `/` | the directory and everything under it — the same as appending `**` |
| anything else | itself, literally |

There are no character classes, no braces, no negation and no `?`. Patterns are always relative to
the repository root.

**A page with no `Supported paths` section supports any path** — the behaviour from before the
section existed, and the honest answer for a methodology whose scope genuinely cannot be written as
a set of paths. A section that exists and declares *nothing* is read the other way, as a page whose
author opened the subject in order to constrain it and then named none; the refusal says so, because
that is almost certainly a mistake in the page rather than a methodology covering nothing.

What follows from the declaration is asymmetric on purpose:

| Moment | No section | Path matches | No match |
|---|---|---|---|
| `grant` | accepted | accepted | **refused**, printing every pattern with its gloss. Overridable with `--force`, or by answering the question it asks when you are at a terminal |
| `check-grants` | silent | silent | a **warning**, never a failure |
| `who-grants`, `granted-by`, `granted-with` | listed | listed | listed and marked `(!)`, with one legend line at the end of the run |

Refusing at grant time is cheap and catches the mistake while its author is standing there. Failing
at check time would break a repository over a page somebody else edited, which is not something a
repository's own continuous integration should be able to suffer.

`--force` has no configuration-file or environment tier, unlike every *setting* in this command. It
is not a setting: it is a per-invocation override of a refusal, and written into a configuration
file it would permanently disable the very check it overrides, for grants nobody was present to
consider. `grant` also refuses when the methodology page cannot be resolved at all — signing under
a text this run could not read is exactly what a signature must not mean — and `--force` overrides
that too.

### What a page pins, and checking it

A methodology page is prose, and prose cites things: a diagram, a specification, another document. A
grant made under that page is in part a claim about what those said, so a page may pin each of them
by hash in an ordinary reStructuredText comment:

```rst
.. image:: assets/pipeline.svg

.. sha256: assets/pipeline.svg 3f2a1b…c9

See the `extraction pipeline <https://example.org/pipeline.pdf>`_ for the derivation.

.. sha256: https://example.org/pipeline.pdf 8ad4…01
```

`..` followed by text that is not a directive is a comment in every reStructuredText parser: it
renders as nothing, breaks no build, and reads plainly in the source.

**The pins are not a second trust root.** They are lines of the page, so the page's own `sha256` —
the one your document records — already commits to every one of them. Verifying them is an optional
*deepening*, never a separate thing to trust:

```console
$ freeports-validate check-methodology "basic check"             # fetches and compares each pin
$ freeports-validate check-methodology "basic check" --no-deep   # the page's own hash, and no more
$ freeports-validate check-grants                                # the same, every adopted methodology
```

`check-methodology` needs no key. It reports the URI the name resolved from, the page's hash, and
three counts: **pinned** targets, **unpinned** ones (a count rather than an error — most prose links
are incidental) and **stale** pins, which name something the page no longer cites at all. A stale
pin is a claim left behind by an edit, and only the person who made the edit can say whether the
claim went with it.

**Following the pins is the default.** It is the question a reader is really asking — not "does this
page still hash the same" but "do the things it relies on still say what its author read" — and it
should not be the one you have to know to ask for. It costs one fetch per pinned resource, so there
is a way out: `--no-deep` (or `--shallow`) for one run, `validate.deep: false` in the configuration
file, `FREEPORTS_VALIDATE_DEEP=0` in the environment. Turning it off is a decision about cost, not a
claim that the page is untrustworthy: the page's own hash still matches, and that is what the
document recorded.

To re-pin a page you are the author of:

```console
$ freeports-validate refresh-links docs/source/validation/methodologies/basic_check.rst
```

It fetches everything the page cites, rewrites its `.. sha256:` comments in place, and is idempotent
— running it twice changes nothing. It refuses a URI, and it refuses a copy sitting inside the
resolver's own cache, because editing one of those would produce a text whose hash matches nothing
anybody published. When it does write, it says loudly, in red, that the page's own hash has changed
and that **every grant made under it is now invalid**. That is the mechanism working, and it is why
this is a separate command rather than a step of `update`.

Two rules govern what it keeps. A pin whose target the run could not reach keeps exactly the hash it
had — turning a network failure into a withdrawn claim would be the quietest possible way to lose
one. A pin naming something the page has stopped citing is dropped.

### Checking grants
```console
$ freeports-validate check-grants                    # your own document
$ freeports-validate check-grants someone_else.yaml  # another contributor's
```

It reports, item by item: the schema, the signature, the general-methodology version, the hash of
every adopted methodology, and the hash of every granted file.

A run ends in one of **three** states, and the exit status distinguishes all three. Two would not
be enough, because "everything I checked was fine" and "I checked everything and it was fine" are
different sentences, and only the second is a grant verified:

| Exit | What it means |
|---|---|
| `0` | every claim was compared against what it names, and every comparison held |
| `1` | something was compared and did not hold: a signature, a version, a methodology hash, a file hash |
| `3` | nothing was found wrong, and not everything was looked at |

The third is the one worth dwelling on. A methodology page that could not be fetched leaves every
grant made under it **uncompared** — neither confirmed nor disproved. Counting that as a failure
would accuse a granter over a network outage, so it is not counted; reporting it as a pass would
launder "I could not look" into "I looked and it was fine", which is the one sentence a validation
tool must never say. So the run says, in as many words, that those grants are *not vouched for by
this run* and should be treated as ungranted until a run that could read the pages says otherwise.

`--offline` makes that state deliberate rather than accidental: it answers from the cache where it
can, and only what it could not answer leaves the run inconclusive.

One thing it says is a warning and never any of the three:

| | |
|---|---|
| a file granted outside the paths its methodology declares | a warning. Failing here would break a repository over a page somebody else edited |

The explanations below are printed on a **green** run too: a warning nobody explains is a warning
nobody acts on.

Those lines name a discrepancy rather than explain one, so a failing run closes with a short section
saying what each *kind* of failure it hit can mean — that an invalid signature is far more often a
public key missing from your keyring than a document somebody tampered with, that a file hash
mismatch is the mechanism working rather than breaking, that a changed methodology page invalidates
the claims made under it because they were claims about that text. For the two hash mismatches that
involve a resolved page — a methodology's, and the general methodology behind `version` — the first
cause it names is a **differently-configured source**, ahead of the page having changed, and it
prints the sources in use and the URI the name resolved to here, so you can compare them with the
granter's. It is printed once for the whole
run rather than once per document, and only for the kinds that actually occurred; a run with nothing
wrong prints none of it.

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

All three annotate a grant made outside the paths its methodology declares with `(!)`, and print one
legend line at the end of the run explaining the mark. Such a grant is listed rather than hidden:
somebody made that claim, and the reader is told under what.

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

### The coverage report

Everything above answers a question about one thing — this file, this person, this methodology. The
report answers the question about the repository: **what has been vouched for here, by whom, under
what, and does it still hold?**

It is two subcommands, and the split is the design:

```console
$ freeports-validate collect                                    # the model, as JSON
$ freeports-validate report --format html     --out coverage.html
$ freeports-validate report --format badges   --out docs/badges/
$ freeports-validate report --format markdown --out README.md
$ freeports-validate report --format rst      --out README.rst
$ freeports-validate report                                     # the model again, the default
```

`collect` walks `validation/*.yaml` once and establishes the facts; every rendering is a **pure
function of that JSON**. So a question none of the renderings anticipated is a `jq` expression away
rather than a feature request:

```console
$ freeports-validate collect | jq '.grants[] | select(.state != "current")'
$ freeports-validate collect | jq '.methodologies[] | {name, coverage}'
```

Neither needs a key. A report is most often wanted by somebody who did not write the repository.

#### What the model holds

| Key | |
|---|---|
| `sources`, `general_methodology` | what this run resolved from, and the text every document's `version` is a claim about |
| `contributors` | one per document: identity, `schema`, `signature`, `version`, how many grants, and whether it is `counted` |
| `methodologies` | name, resolution state, resolved URI and hash, the patterns it declares, one `adoptions` entry per document that adopted it, and its coverage |
| `grants` | one per granted file: path, recorded hash, `state`, and `in_scope` |
| `coverage`, `totals`, `status` | the repository-wide figures, and one word for the run |

Three of those fields are three-valued rather than two, for the same reason each time — a question
that could not be *asked* must not be answered:

| Field | |
|---|---|
| `grants[].state` | `current`, `changed`, `missing` |
| `grants[].in_scope` | `true`, `false`, or `null` when the methodology page could not be resolved |
| `status` | `passing`, `failing`, or `inconclusive` — something could not be reached, and nothing was disproved |

#### How the percentage is arrived at

**A methodology that declares no paths contributes no denominator.** There is nothing it could be
complete against, and inventing one would put a meaningless percentage on a badge. For one that does
declare paths, the denominator is what its patterns match in the working tree — the files it *could*
cover — and the numerator is how many of those somebody has granted under it.

**A document that is not intact counts toward nothing.** Coverage is a statement about assurance, and
a document whose signature is missing or wrong, or whose `version` is stale, asserts nothing. Its
grants are still listed, with the state of the document they came from: an uncounted grant that were
also invisible would be a report quietly disagreeing with the repository it describes.

**The repository-wide figure is a union, not a sum.** Adding the per-methodology fractions together
would count a file twice as soon as two methodologies declared it, and a badge reading 150% is worse
than no badge.

#### The renderings

| `--format` | `--out` | |
|---|---|---|
| `json` | optional | the model. The default |
| `html` | a file | one self-contained page — inline style and script, no fetch of any kind — with the three viewpoints as tabs, matching `who-grants` / `granted-by` / `granted-with`, a filter box, and every methodology name linking to the text it resolved from |
| `badges` | a **directory** | `grants-total`, `grants-coverage` and `check-grants`, each as an SVG drawn locally in the shields style and as the shields.io *endpoint* JSON for anyone who would rather fetch |
| `markdown`, `rst` | a file | one of seven tables, rewritten in place between two markers |

The Markdown and reStructuredText renderings write **into** a file somebody else wrote, between
markers you paste where the table should go:

```markdown
<!-- freeports-validate:begin -->
<!-- freeports-validate:end -->
```

```rst
.. freeports-validate:begin
.. freeports-validate:end
```

Rewriting is idempotent: running it twice changes nothing. A file with no marker pair is an **error**
naming the markers, never an append — where a coverage table belongs in a README is a judgement
about that README, and a command that guessed would put one under a licence notice and then keep it
there for ever.

Nothing the report produces carries a timestamp. A date would make every regeneration a diff saying
nothing had changed, which is how a generated block stops being regenerated. What a reader wants to
know is what the repository says now, which running the command again tells them.

#### The seven tables

The HTML page shows three viewpoints as tabs. A Markdown or reStructuredText file cannot have tabs,
so it holds **one** table, chosen with `--table`.

There are seven. The repository has three dimensions — file, contributor, methodology — and a lookup
is a choice of two of them: group by the first, list the second. Taken in every order that is six,
which is exactly what the three lookup subcommands are with and without their flag. The seventh is
the summary, and it is the default, so every invocation written before `--table` existed still means
what it meant.

| `--table` | The same view as |
|---|---|
| `summary` *(default)* | one line per methodology: its grants, its coverage, who adopted it |
| `file-contributor` | `who-grants <file>` |
| `file-methodology` | `who-grants -m <file>` |
| `contributor-methodology` | `granted-by <contributor>` |
| `contributor-file` | `granted-by -f <contributor>` |
| `methodology-file` | `granted-with <methodology>` |
| `methodology-contributor` | `granted-with -c <methodology>` |

The name reads as *grouped by the first, listing the second*, and each table says in its own heading
which invocation it is the written-down form of — so a reader who has learned the lookups does not
have to learn a second vocabulary for the same repository.

Each is three columns, one row per distinct pair, with the group key repeated down the first column
rather than blanked. Joining the other dimension into one cell is what the lookup subcommands print
on a terminal, and it stops being readable the moment a methodology covers several hundred files. The
third column is the state of the grants that pair stands for — almost always one word, and two when
two documents disagree about the same file.

The `(!)` and `(?)` marks are the ones `granted-by` and its two siblings already print, and they go
on the **listed** item, never on the group key: what is out of scope is the *pair*, so marking a
heading would split one methodology into two the moment one of its grants left scope. A legend line
appears under the table only when a mark actually appears on it.

#### Rendering several things out of one walk

A repository that keeps a README, six tables and three badges current has nine artefacts to write,
and collecting nine times would resolve every methodology page nine times for an answer that cannot
have changed in between. `--model` renders a model gathered earlier instead of walking again:

```console
$ freeports-validate collect > model.json
$ freeports-validate report --model model.json --format badges   --out validation/report/badges/
$ freeports-validate report --model model.json --format markdown --out README.md
$ freeports-validate report --model model.json --format markdown \
      --table methodology-file --out validation/report/methodology-file.md
```

`--model -` reads the model from a pipe, so `collect | report --model -` works as well. A `--model`
naming a file that is not there is an **error**, deliberately not a reason to walk the repository:
a run that quietly answered a different question from the one asked is the failure this whole
command exists to prevent.

#### What a repository gets for free

`freeports-dev init-format-repo` creates all nine of those, already filled, plus the hook that keeps
them current — see {ref}`what-init-format-repo-writes`. So the usual answer to "how do I set this
up" is that it is already set up.

### Where the validation files live
| Path | What |
|---|---|
| `<repo>/validation/<name>.yaml` | one validation document per contributor |
| `<repo>/package.yaml` → `info.validation_sha256` | ties the repository to the document vouching for it |
| the installed package's `lib/document.schema.json` | the schema every document is checked against |
| `${XDG_CACHE_HOME:-$HOME/.cache}/freeports-validate/` | every methodology body ever fetched, keyed by the URI's hash and then by its own |

The methodology pages are **not** in that list, and that is the point. They ship neither with the
repository nor with the tool: they are resolved from the sources you configure, so the text a grant
refers to is a document at an address its granter chose. The pages this project publishes are in
{doc}`the validation section <../../validation/index>` of this site, which is exactly what the
default source fetches — and, being hashed artefacts, they cannot be edited as ordinary prose.
