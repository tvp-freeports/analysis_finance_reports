# Being trusted with the numbers

Extracting data from documents designed for human eyes is not exact. Any tool that says otherwise is
misrepresenting itself, and a tool whose output feeds decisions about money owes its users something
better than either a disclaimer or a boast.

## The two easy answers, and why neither is taken

**Take no responsibility.** Ship a licence that makes checking the output entirely the user's
problem. Cheap, fast, and it moves the whole burden onto the person least able to carry it — someone
who would have to re-read the PDFs to check, which is what they came here to avoid.

**Take total responsibility.** Publish only what has been verified to a standard nobody can dispute.
Also unworkable: it slows the work to the speed of manual review and ends up reproducing data that
commercial databases already sell, months later.

This project sits near the second end without pretending to reach it. The commitment is not "the
output is correct" but: **the method is published, the claims are attributable, and the limits are
stated**. A user who wants to rely on a number can find out how it was produced, who vouched for it,
and under which protocol — and can then decide, which is a decision they can only make if the
information exists.

## How a claim is recorded

The mechanism is deliberately small. It is a **grant**: a record that a named contributor vouches for
specific files, under a specific published methodology, at a specific content hash.

A contributor has one **validation document**, a YAML file in the repository's `validation/`
directory, holding: who they are and their public key; which methodologies they are using, each
pinned by the hash of the page describing it; the files granted under each, each pinned by the hash
of its own content; and a cryptographic signature over the whole thing.

Everything in that structure is content-addressed, and that is the whole idea. If a granted file
changes, its hash no longer matches and the grant visibly no longer applies to what is on disk. If a
*methodology page* changes, every grant that cited it is invalidated, because the claim was made
about that text and the text is now different. The document itself names the version of the general
methodology it was written under, by the hash of that page — so the meaning of the entries cannot
drift out from under them either.

This has a practical consequence worth stating plainly, since it catches people: **the validation
pages in this documentation are not editable prose.** Correcting a typo in one of them invalidates
signed grants in repositories maintained by other people. Changing them is a deliberate operation —
re-granting and re-signing — not an act of tidying.

## A methodology is a document at a location you chose to trust

A methodology page does not ship with the tool, and it does not live in the repository being
vouched for. It is **published somewhere**, and each person who uses it configures where they read
it from — one or more *sources*, patterns like
`https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt`, in which a `*` stands for the
methodology's name.

The pages used to travel inside the installed command, and the reason for moving them is worth
following, because the same reasoning explains what a grant means now. When the text lived in the
package, upgrading `freeports-validate` silently changed what every grant in every repository
referred to: a claim you made in March could come to mean something else in April because your
package manager updated something. The text a claim is about should not be chosen by a package
manager.

So it is chosen by you. And the consequence — the honest half of the trade — is that whoever
publishes a methodology can now invalidate grants made under it, by editing the page. That is not a
flaw that survived review; it is the correct shape of the thing. A methodology *is* a text, a grant
*is* a claim about that text, and the person who controls the text therefore controls what the claim
meant. Making that visible is better than hiding it inside a version number.

### The source is a contract, and it is deliberately not recorded

A validation document stores a methodology's **name** and its **sha256**. It stores nothing about
where either came from.

That looks like a missing field and is a decision. The source is an agreement between the person who
granted and the person who verifies, and each of them writes it in their own configuration — the
granter chose which publication of a methodology they were working under, and the verifier chooses
which publication they are checking against. Recording the granter's source inside the document
would make the document assert something about the verifier's setup that it has no standing to
assert, and would quietly turn a shared name into a URL that has to keep working for ever.

The price is that a hash mismatch is ambiguous. It may mean the page was rewritten; it may mean the
two of you were reading two different publications of the same methodology. The tool pays that price
in prose rather than in schema: a mismatch names a differently-configured source as the first likely
cause, and prints the sources in use with the URI the name resolved to, so two people can compare
one line and see which of the two it was.

```console
$ freeports-validate sources
```

### What a methodology may say about itself

Two optional things on the page are read by the tool rather than only by people, and both exist to
make a grant more legible rather than to constrain it.

A **`Supported paths`** section declares which repository paths the methodology covers, each with a
sentence saying what vouching for such a file claims. A page that has no such section covers any
path, which is the honest answer for a methodology — `agreement and good faith` is the example —
whose scope cannot be written as a set of paths. Where a section exists and a grant falls outside
it, `grant` refuses while its author is still standing there, and `check-grants` only warns: a
repository must not go red because somebody else edited a page it adopts.

A page may also **pin what it cites** — a diagram, a specification, another document — by hash, in
ordinary reStructuredText comments. Those lines are part of the page, so the page's own hash already
commits to them; following them is a deepening, never a second thing to trust. A check does follow
them by default, because "do the things this page relies on still say what its author read" is the
question actually being asked; `--no-deep` turns that off where the fetch per pinned resource is
what you cannot afford.

{doc}`formats/writing-a-methodology` is the guide to writing one.

## The three published methodologies
A methodology says what a person actually did before vouching. Three are published today, and they
differ in how much verification they claim, not in how much they promise:

**Basic check** — the output files were generated, they look reasonable, and a human has actually
looked at them. It is a low bar, and it is stated as a low bar. Its value is that it is *honest*
about being one, and that someone's name is attached to it.

**Golden standard** — basic check plus manual verification of the intermediate blocks: every unit
the extraction saw was reviewed, and the claim includes that nothing relevant was missed. Expensive,
and correspondingly rare.

**Agreement and good faith** — used for assertions rather than data: a contributor states that they
have read a claim, understood it, judged it within their competence, and agree with it. It is how a
statement that cannot be checked by running a program still gets a name attached.

The full text of each — scope, protocol steps, what is and is not certified — is in
{doc}`the validation section <../validation/index>`, and that text, not this summary, is what a
grant refers to.

## Working with grants

```console
$ freeports-validate create-document      # once, per contributor
$ freeports-validate grant <files…>       # vouch, under a methodology
$ freeports-validate sign-document        # sign it
$ freeports-validate check-grants         # verify: signatures valid, hashes current
```

`check-grants` is the one to run before trusting a repository you did not write, and in continuous
integration — and, unlike the four that write your document, it needs **no key of your own**: asked
without one it checks every document in the repository against your keyring, which is exactly the
question an auditor and a CI job are asking. It answers a narrow question — are these grants still about these files — and it is not
a substitute for the tests passing, any more than the tests are a substitute for it. One says the
code does what it did yesterday; the other says a person put their name to the result.

To find out who stands behind something: `who-grants <file>`, `granted-by <contributor>`,
`granted-with <methodology>`.

`update` refreshes the hashes after a granted file legitimately changed. It re-states the intent to
vouch, and it should be run by someone who has confirmed the change was expected — it is not the way
to silence a `check-grants` failure.

To see the state of a whole repository rather than of one file or one person: `report` renders the
same three viewpoints as a page, as badges, or as a table it rewrites into a README, out of a model
`collect` prints as JSON for anything the renderings did not anticipate.

{doc}`formats/tooling` is the operating manual for all of this: what to install, where methodology
pages are resolved from, how to generate and register the GPG key the whole mechanism hangs on, and
what each subcommand does to the document. {doc}`formats/writing-a-methodology` is the other side of
it — writing the text that grants are made under.

## What this does not give you

It does not make the extraction correct. It makes the extraction **accountable**, which is a
different and smaller thing: you can find out what was claimed, by whom, on what basis, and about
exactly which bytes. Where that is not enough for your purpose, the honest answer is that it is not
enough, and the {doc}`validation section <../validation/index>` is written to let you determine that
for yourself rather than to reassure you.
