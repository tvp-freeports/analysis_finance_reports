# The freeports whitepaper

`freeports` turns financial reports published as PDF into tables you can compute on.

Funds, and the companies that manage them, are required to disclose what they hold, what their
assets are worth, who manages them and how they classify themselves under sustainability
regulation. They do disclose it — as an annual report, in PDF, laid out however the issuer's
typesetter chose. The obligation is on the *publication*, not on the *shape*, so the same fact is
printed in several hundred mutually incompatible ways. Anyone who wants to compare across issuers
has to read them by hand, buy the data back from a vendor who read them by hand, or write a program
per issuer.

This project is the third option, made maintainable: an engine that knows nothing about any
particular report, plus **formats** — small, separately maintained recipes, one per report layout —
that tell it where to look. The engine is the part that does not change; the formats are the part
the community grows.

<!-- The toctree is hidden and sits here, above the first section, on purpose: placed at the
     end of the page it would be nested under whatever section came last, which is neither what
     the hierarchy is nor what the side panel should show. -->

```{toctree}
:maxdepth: 2

problem
usage/index
formats/index
design/index
input-db
validation
```

## The four areas of the whitepaper
| Area | Is | For |
|---|---|---|
| {doc}`problem` and {doc}`validation` | what the project is for, and what we do and refuse to do about being trusted | anyone, no technical background needed |
| {doc}`usage/index` | installing it, running it, configuring it | someone who wants results |
| {doc}`formats/index` | adding support for a report layout, and the repository that holds it | someone extending it |
| {doc}`design/index` | why the algorithm has the shape it has | someone who wants to argue with it |

Plus {doc}`input-db`, which is how you say which companies a run is looking for — needed by everyone
who runs it and maintained by whoever curates the lists.

## Reading paths, by what you need
**Assessing whether to rely on this.** {doc}`problem`, then {doc}`validation`. Two chapters, no
technical background, and they are honest about the limits rather than reassuring.

**Getting results out of it.** {doc}`usage/index` in order — install, the two inputs, a first run —
then {doc}`usage/configuration/index` when you tire of long command lines.

**Supporting a report nobody supports yet.** {doc}`formats/writing-a-format` end to end, with
{doc}`formats/dev-loop` open beside it. {doc}`design/index` when you want to know why a thing works
the way it does rather than just how.

**Arguing with a decision.** {doc}`design/index`, and in particular {doc}`design/limits`, which
lists what is accepted, what is broken, and what is designed and not built. If the reason for a
choice no longer holds, that is worth knowing — and the choices about *technology* rather than
*algorithm* are in {doc}`the implementation notes <../dev/implementation-notes>`.

## The whole project in one paragraph
A document becomes a sequence of pages, and **the page is the unit of work**: it is assumed to carry
the context needed to understand what is on it. Pages are classified — per document — then poured
into one **schedule** of steps, where each step's results filter the next. Every page of a class goes
through a **bundle of pipelines**, each three segments long: what is on this page, does it concern
us, what does it mean. What comes out is an entity, or a **promise** for a value the page could not
know. At the end, promises resolve, entities accumulate, and the whole run writes **one** set of
tables. Everything else — parallelism, testability, localised failures — follows from that first
assumption.
