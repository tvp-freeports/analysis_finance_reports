# Contributing a grant

You have read a file — a format's reference output, or a page of prose about this software — and you
are willing to say so under a published methodology. This section is that act: what it means, how it
is recorded, and what keeps it honest.

```{toctree}
:maxdepth: 1
:hidden:

writing-a-methodology
```

## What a grant is, and where the pieces live
| You want | Read |
|---|---|
| to know why any of this is worth doing | {doc}`../institutional/why-trust-a-grant` |
| the published methodologies themselves | {doc}`../../validation/index` |
| every option of the command | {doc}`../../reference/cli/freeports-validate` |
| how a methodology name resolves to a text | {doc}`../../reference/cli/methodology-sources` |
| to write and publish a methodology of your own | {doc}`writing-a-methodology` |
| what this repository's grants currently claim | {doc}`../../validation/report/index` |

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

{doc}`../../reference/cli/freeports-validate` is the operating manual for all of this: what to install, where methodology
pages are resolved from, how to generate and register the GPG key the whole mechanism hangs on, and
what each subcommand does to the document. {doc}`writing-a-methodology` is the other side of
it — writing the text that grants are made under.
