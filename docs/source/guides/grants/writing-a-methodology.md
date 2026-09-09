# Writing a methodology

A methodology is a page of prose. Nothing about it is executable, nothing about it is checked by a
program, and the tool that uses it never reads a word of it except to find one optional section.

That is not a limitation to work around. It is the whole idea, and this chapter is about writing one
well.

## What a methodology is for

When somebody grants a file, what they sign is roughly: *I applied the protocol described in this
text to this file, and I stand behind the result.* The methodology is that text. So the question a
methodology page has to answer is not "what should the tool check?" — the tool checks hashes and
signatures and nothing else — but:

> **What is the person who grants a file under this methodology claiming, and what are they not
> claiming?**

A reader downstream will one day look at a `.csv` in a formats repository, see that three people
vouched for it under *basic check*, and decide how much that is worth. What decides that is your
page. If it says something a careful person can honestly do and honestly report, the grant means
something. If it is a checklist nobody could truthfully complete, the grant is theatre, and the
mechanism around it — hashes, signatures, sources — is theatre too.

```{admonition} The one test that matters
:class: tip

Read your protocol steps and ask: *could a competent, honest person do exactly this, in an
afternoon, and know whether they had done it?* A step that cannot be honestly completed does not
raise the trust level of the methodology. It lowers it, because the grants made under it were made
by people who ticked it anyway.
```

Two consequences follow, and both are worth saying out loud on the page itself:

**A methodology is not a quality gate.** It does not say the data is right. It says what a person
did and what they concluded. `basic check` says somebody ran the thing and looked at the output;
`golden standard` says somebody checked every value against the source document. Neither says the
extraction is correct, because no human procedure says that.

**Weak methodologies are useful.** The temptation, writing a first methodology, is to make it
demanding so that the grant sounds impressive. Resist it. A weak claim that is true is worth more
than a strong claim that is aspirational, and a repository is far better served by a hundred honest
`basic check` grants than by three `golden standard` grants nobody could really have earned.

## The anatomy of a page

The three published methodologies share a shape, and a new one should follow it. Only the last row
is read by a program; every other section exists for the person deciding whether to grant, and for
the person downstream deciding what the grant is worth.

| Section | Answers |
|---|---|
| **Purpose** | in one or two sentences, what a grant under this methodology asserts |
| **Scope** | what kinds of thing it applies to, in prose |
| **Protocol steps** | what the granter actually does, in order, concretely enough to follow |
| **Trust level** | how strong the resulting claim is, said plainly and without flattering it |
| **Applicable context** | when to reach for this one rather than another |
| **Limitations** | what a grant under it does **not** establish |
| **Supported paths** | *optional, and machine-read* — the repository paths it covers, each with a gloss |

The order is a reading order, not a rule. What matters is that **Limitations** is there and is
honest: it is the section a downstream reader trusts you for, and a page without one reads like
marketing.

Here is the skeleton, in reStructuredText, which is the format a methodology page is written in:

```rst
=======================
Basic Check Methodology
=======================

**Purpose**
The basic_check methodology provides a lightweight validation that ensures program outputs are
generated correctly and appear reasonable at first glance.

**Scope**
This methodology covers files in the test output directories and certifies that the program
generated them, that their content follows the expected shape, and that a human has looked at
them.

**Protocol Steps**

1. **File Generation Check**: verify that all expected output files are created by the program
2. **Basic Content Validation**: check that CSV files contain data with the expected columns
3. **Human Visual Inspection**: quick review of the output for obvious anomalies

**Trust Level**
This methodology provides **weak certification** — it indicates the program functions correctly on
test data and produces outputs that appear reasonable, but does not guarantee complete accuracy.

**Limitations**
It establishes nothing about values that are wrong but plausible.
```

## Declaring the paths it covers

Everything above is prose for people. `Supported paths` is the one section a program reads — and
even there, what it reads is only the patterns; the prose under each of them is shown to the reader
untouched, never interpreted.

```rst
Supported paths
===============

Vouching for a file under this methodology means the protocol above was applied to it. What that
means concretely depends on what the file is, so each path this methodology covers says it:

``tests/formats/**/out/*.csv``
    The reference output of a format's test suite in a **formats repository**. A basic check here
    means the run that produced the file completed, the columns are the expected ones, and a human
    has looked at the values for obvious nonsense.

``tests/formats/**/pages/**``
    The per-page fixtures of the same suite. A basic check here means the page classification was
    confirmed by eye against the PDF.
```

It is an ordinary, **visible** section, and each entry is a definition-list term that is a single
inline literal with an indented gloss under it. Anything else in the section is prose and is
ignored. Two deliberate non-choices are worth knowing: it is not a hidden comment block, because
that would put the declaration where a reader of the published page could not see it; and it is not
a custom Sphinx directive, because that would break every plain-reStructuredText consumer —
including the tool itself, which fetches the page as text from wherever you published it.

### The grammar

Four tokens, and everything else is literal:

| Token | Matches |
|---|---|
| `*` | any part of **one** path segment; never crosses a `/` |
| `**` | zero or more whole segments |
| a trailing `/` | the directory and everything under it — the same as appending `**` |
| anything else | itself |

No character classes, no braces, no negation, no `?`. Patterns are always relative to the repository
root, and a leading `/` is ignored rather than refused.

The grammar is this small on purpose. These patterns are read by people deciding whether to trust a
grant, and a pattern language that *looked* like a shell glob without being one would be worse than
having none: the reader would believe something the tool does not do.

`**` is not decoration, and the commonest mistake is reaching for `*` where the real tree needs it.
A formats repository may nest a **variant** between the format and its outputs, and may equally
not — both layouts are ordinary, and a real repository contains a mixture:

```text
tests/formats/ASTERIA-EN23/out/funds.csv          no variant
tests/formats/ARCA-IT24/1/out/funds.csv           a variant, "1"
              ^^^^^^^^^^ ^
              format     variant
```

`*` matches exactly one segment, so `tests/formats/*/out/*.csv` covers the first and misses the
second, and `tests/formats/*/*/out/*.csv` does precisely the reverse. Neither is what you meant.
`tests/formats/**/out/*.csv` covers both, because `**` is *zero or more* segments — which is the
whole reason the grammar has a token for that and not merely for "one".

Check yours against real paths before publishing, from the repository the patterns are about:

```console
$ python3 lib/pathmatch.py match 'tests/formats/**/out/*.csv' \
      tests/formats/ASTERIA-EN23/out/funds.csv
$ python3 lib/pathmatch.py match 'tests/formats/**/out/*.csv' \
      tests/formats/ARCA-IT24/1/out/funds.csv
```

It prints nothing and answers in its exit status, like `test`. To see everything a pattern would
cover in a repository as it stands — which is also the denominator of its coverage figure:

```console
$ python3 lib/pathmatch.py expand 'tests/formats/**/out/*.csv' ~/work/my-formats
```

### What follows from declaring, and from not declaring

**A page with no `Supported paths` section supports any path.** That is the behaviour from before
the section existed, and it is the honest answer for a methodology whose scope genuinely cannot be
written as a set of paths. Leaving the section out is a legitimate choice, not an omission.

A section that exists and declares *nothing* is read the other way — as a page whose author opened
the subject in order to constrain it and then named none — and `grant` says so, because that is
almost certainly a mistake in the page rather than a methodology that covers nothing.

Where a grant names a path outside the declared set, the three moments behave differently on
purpose:

| Moment | What happens |
|---|---|
| `grant` | **refused**, printing every pattern with its gloss. Overridable with `--force`, or by answering the question it asks at a terminal |
| `check-grants` | a **warning**, never a failure |
| the three lookups | listed, and marked `(!)` |

Refusing at grant time is cheap and catches the mistake while its author is standing there. Failing
at check time would break somebody's repository because *you* edited your page, which is not a power
a methodology author should have.

### The same path in two repositories

`tests/formats/` exists in a formats repository and in the engine's own repository, and it means
different things in the two. A pattern cannot tell them apart, and the tool does not try.

**Resolve it in the gloss.** This is the reason the prose under each pattern is required reading
rather than decoration:

```rst
``tests/formats/**/out/*.csv``
    The reference output of a format's test suite **in a formats repository** — one produced by
    ``freeports-dev make-tests`` and used by ``freeports-dev test`` as the expected result. This
    methodology does *not* cover the engine repository's own files of that name, which are fixtures
    of the extraction library rather than of a format.
```

A reader who is about to grant sees that sentence in the refusal message if their path does not
match, and sees it on the published page if it does. That is where the ambiguity gets resolved: by a
person having written down which one they meant.

## A worked example: a methodology for assertions

The three published methodologies do not all look alike, and the interesting one is
**`agreement and good faith`**, because what it grants is not a test output at all.

Its subject is an **assertion**: a statement in the documentation. Something like *"the extraction
algorithm does not silently drop rows it cannot parse"* — a claim about the software, written as
prose, which somebody can read, understand and agree with. Granting it does not mean you ran
anything. It means you read the statement, you understood it within your own technical competence,
and you are willing to put your name to agreeing with it.

Read its trust level as its author wrote it:

> This methodology provides **personal certification** — it represents an individual's good-faith
> agreement based on their current technical understanding, but does not guarantee objective truth
> or comprehensive verification.

That sentence is the whole methodology in miniature, and it is a model of the kind of honesty the
section asks for. It does not say the assertion is true. It says a named person, with stated
limitations, agreed with it.

Its `Supported paths` therefore look nothing like the other two. There are no test outputs to name:

```rst
Supported paths
===============

``docs/source/validation/assertions/**``
    An assertion in **this project's own documentation repository** — a statement about the
    software, published as prose, which a contributor can read and agree with. Granting one is a
    claim about the *statement*, not about any file the software produced. Another repository
    publishing its own assertions would keep them elsewhere, and this pattern does not reach them.
```

Three things in that gloss are worth copying:

1. it says which repository the path is in, because the same path elsewhere is not the same thing;
2. it says what the grant is a claim **about** — the statement, not an output — which is the
   distinction that makes this methodology different from the other two;
3. it says what it does **not** reach, so a reader who came looking for their own assertions is told
   why they are not covered rather than left to guess.

Compare with `basic check`, whose patterns name test outputs and whose gloss talks about a program
having run. Same section, same grammar, and two completely different kinds of claim — which is
exactly what a methodology is for.

## Pinning what the page cites

A methodology page is prose, and prose cites things: a diagram, a specification, another document. A
grant made under the page is in part a claim about what those things said. So the page can pin each
of them by hash, in an ordinary reStructuredText comment:

```rst
.. image:: assets/pipeline.svg

.. sha256: assets/pipeline.svg 3f2a1bd4e5c6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1c9

See the `extraction pipeline <https://example.org/pipeline.pdf>`_ for the derivation.

.. sha256: https://example.org/pipeline.pdf 8ad4b1c2d3e4f5061728394a5b6c7d8e9f0a1b2c3d4e5f60718293a4b5c6d701
```

`..` followed by text that is not a directive is a comment in every reStructuredText parser: it
renders as nothing, breaks no build, and reads plainly in the source. The target is either an
absolute URI or a path relative to the page itself.

**These are not a second thing to trust.** They are lines of the page, so the page's own `sha256` —
the one every validation document records — already commits to every one of them. Following them is
a deepening, and it happens by default:

```console
$ freeports-validate check-methodology "basic check"            # fetches and compares each pin
$ freeports-validate check-methodology "basic check" --no-deep  # the page's own hash, and no more
```

You do not write those lines by hand. Let the tool fetch and hash everything the page cites:

```console
$ freeports-validate refresh-links docs/source/validation/methodologies/basic_check.rst
```

It is idempotent, it refuses to touch a page it did not fetch from your own filesystem, and when it
does write it tells you loudly that the page's hash has changed — which is the next section.

## Publishing it, and what publishing commits you to

A methodology becomes usable the moment somebody's `validate.sources` can resolve it. A source is a
pattern with exactly one `*`, standing for the methodology's relative name:

```yaml
validate:
  sources:
    - https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt
    - https://github.com/me/my-methodologies/blob/main/*.rst
```

The name `my methodology` is looked for at `methodologies/my_methodology` under each source in turn,
so a published tree looks like this:

```text
general_methodology.rst
methodologies/
    my_methodology.rst
```

Order is priority, and the arrangement above is the common one: the published documentation for the
methodologies everybody shares, plus your own for the ones you wrote. `freeports-validate sources`
shows what each name resolves to and warns where one source shadows another.

{doc}`../../reference/cli/index` has the mechanics — the grammar in full, the GitHub rewrite, the cache, offline
behaviour. What belongs here is the obligation that publishing creates:

```{warning}
**Editing a published methodology invalidates every grant made under it — everywhere.**

A validation document records the `sha256` of the text its grants were made under. Change one
character of the page and that hash changes, so every document that adopted it now points at a text
that no longer exists, in every repository you have never heard of. `check-grants` reports it, and
the only honest remedies are `update methodology` — which **drops** every file granted under it,
because those claims were about the old text — or reverting your edit.

That is the mechanism working, not breaking. But it means a published methodology is a commitment,
and the time to get the wording right is before anybody adopts it.
```

Two practical consequences:

**Version by name, not by edit.** If a methodology needs to change substantively, publish
`my_methodology_2` beside the first and let people adopt it deliberately. Editing in place asks
everyone who trusted you to re-do work they already did.

**Typos are not free.** There is no such thing as a cosmetic change to a hashed artefact. Proof-read
before publishing, and expect to fix mistakes by publishing a new name rather than by correcting the
old one.

## A checklist

1. Write **Purpose**, **Scope**, **Protocol steps**, **Trust level**, **Applicable context** and
   **Limitations**, in prose, for a person.
2. Read the protocol steps back and ask whether an honest person could complete them and know they
   had.
3. Decide whether the scope is a set of paths. If it is not, leave `Supported paths` out.
4. If it is, write each pattern with a gloss that says which repository it means and what vouching
   for such a file claims.
5. Check every pattern against a real path with `pathmatch.py match`.
6. Run `refresh-links` if the page cites anything.
7. Publish it, and point a source at it.
8. Check it resolves, before telling anyone: `freeports-validate check-methodology "my methodology"`.
9. Treat it as frozen from the first grant made under it.
