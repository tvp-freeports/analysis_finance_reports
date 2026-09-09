# Why a grant is worth something

A grant is a signed statement that a named methodology was applied to a specific file. This page is
the argument for why that is worth more than a disclaimer and less than a guarantee — read it
before deciding how much weight to put on the figures. The mechanics of making one are
{doc}`../grants/index`; the published methodologies themselves are
{doc}`../../validation/index`.

## A methodology is a document at a location you chose to trust

A methodology page does not ship with the tool, and it does not live in the repository being
vouched for. It is **published somewhere**, and each person who uses it configures where they read
it from — one or more *sources*, patterns like
`https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt`, in which a `*` stands for the
methodology's name.

The obvious alternative — shipping the pages inside the installed command — is the one thing this
arrangement exists to avoid, and following why explains what a grant means. If the text lived in the
package, upgrading `freeports-validate` would silently change what every grant in every repository
referred to: a claim you made in March could come to mean something else in April because your
package manager updated something. The text a claim is about must not be chosen by a package
manager.

So it is chosen by you. And the consequence — the honest half of the trade — is that whoever
publishes a methodology can invalidate grants made under it, by editing the page. That is not a flaw
that survived review; it is the correct shape of the thing. A methodology *is* a text, a grant *is* a
claim about that text, and the person who controls the text therefore controls what the claim meant.
Making that visible is better than hiding it inside a version number.

## The source is a contract, and it is deliberately not recorded

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

## What a methodology may say about itself

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

{doc}`../grants/writing-a-methodology` is the guide to writing one.

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
{doc}`the validation section <../../validation/index>`, and that text, not this summary, is what a
grant refers to.


## What this does not give you

It does not make the extraction correct. It makes the extraction **accountable**, which is a
different and smaller thing: you can find out what was claimed, by whom, on what basis, and about
exactly which bytes. Where that is not enough for your purpose, the honest answer is that it is not
enough, and the {doc}`validation section <../../validation/index>` is written to let you determine that
for yourself rather than to reassure you.
