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

## Where the rest of this lives
This page is the position, in brief. The argument behind it — why a methodology is a document at an
address you chose to trust, and what each published methodology claims — is
{doc}`../guides/institutional/why-trust-a-grant`. Making a grant yourself is
{doc}`../guides/grants/index`. The methodologies themselves, which are hashed artefacts rather than
ordinary prose, are {doc}`../validation/index`.

## What this does not give you

It does not make the extraction correct. It makes the extraction **accountable**, which is a
different and smaller thing: you can find out what was claimed, by whom, on what basis, and about
exactly which bytes. Where that is not enough for your purpose, the honest answer is that it is not
enough, and the {doc}`validation section <../validation/index>` is written to let you determine that
for yourself rather than to reassure you.
