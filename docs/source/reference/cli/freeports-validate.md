# `freeports-validate`

Everything this command does revolves around one file: your **validation document**, a YAML file at
`<repo>/validation/<your_name>.yaml`, signed by your GPG key. {doc}`../../guides/institutional/why-trust-a-grant` explains why the
mechanism is shaped this way; this section is how to operate it.

## What `freeports-validate` needs installed
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

## Prerequisite: the OpenPGP key

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
({doc}`../configuration/dev-and-validate`):

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


## Creating your validation document

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

## Adopting a methodology, then granting files

A grant is always *a file, under a methodology*, so the methodology has to be adopted first. Three
are published today — `basic check`, `golden standard`, `agreement and good faith` — and
{doc}`../../guides/institutional/why-trust-a-grant` says what each one claims. Names are normalised: underscores become spaces and
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

## What a methodology says it covers

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

## What a page pins, and checking it

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

## Checking grants
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

## Asking who stands behind what

| Command | Lists | Grouped by | Alternative |
|---|---|---|---|
| `who-grants <files…>` | who vouched for these files | contributor | `-m`, by methodology |
| `granted-by <contributor>` | what this person vouched for | methodology | `-f`, by file |
| `granted-with <methodology>` | what was vouched for under this methodology | file | `-c`, by contributor |

A contributor can be named by their name, their email or their key fingerprint — whichever you have.

All three annotate a grant made outside the paths its methodology declares with `(!)`, and print one
legend line at the end of the run explaining the mark. Such a grant is listed rather than hidden:
somebody made that claim, and the reader is told under what.

## After a file legitimately changes

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

## Where the validation files live
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
