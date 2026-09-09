# What freeports is

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
:maxdepth: 1
:hidden:

problem
how-it-works
the-repositories
trust
```

## This section is the fast pass
Four short chapters, no technical background needed for two of them, and each one names the place
where the same subject is treated properly.

| Page | Is | Goes deeper in |
|---|---|---|
| {doc}`problem` | what is being extracted, and why it is harder than scraping | — |
| {doc}`how-it-works` | one run, end to end, in a diagram and a paragraph | {doc}`../reference/design/index` |
| {doc}`the-repositories` | the map: engine, formats, input databases, the site | {doc}`../start/ways-to-contribute` |
| {doc}`trust` | what the project claims about its own output, and what it refuses to claim | {doc}`../guides/institutional/why-trust-a-grant` |

## Reading paths, by what you need
**Assessing whether to rely on this.** {doc}`problem`, then {doc}`trust`, then
{doc}`../guides/institutional/index` — which is written for exactly that question and is honest
about the limits rather than reassuring.

**Getting results out of it.** {doc}`../start/install`, {doc}`../start/first-run`, then
{doc}`../guides/user/index` in order, and {doc}`../reference/configuration/index` when you tire of
long command lines.

**Supporting a report nobody supports yet.** {doc}`../guides/formats/tutorials/first-format` end to
end, with {doc}`../guides/formats/dev-loop` open beside it.

**Arguing with a decision.** {doc}`../reference/design/index`, and in particular
{doc}`../reference/design/limits`, which lists what is accepted, what is broken, and what is
designed and not built. The choices about *technology* rather than *algorithm* are in
{doc}`../guides/engine/advanced/implementation-notes`.
