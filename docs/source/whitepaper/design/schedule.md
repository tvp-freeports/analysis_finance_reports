# The schedule

*Implemented.* A sequence of **steps**, each naming a set of page classes. It is what turns a pool
of classified pages into an order of work.

## Steps exist for one reason

> **The results of one step are the input filter of the next.**

That is what makes a two-stage extraction expressible. Step one finds the funds; step two finds the
managers **of those funds**, because what step one found is what step two filters on.

```text
step[0]   investments                  → funds discovered
             │
             │  results become the filter
             ▼
step[1]   merges, sfdr_classification  → only for those funds
```

In the formats repository this is the `Filter next iteration` column: classes accumulate into the
current step until a row raises the flag, which closes that step and opens a new one
({doc}`../formats/repository`).

Without steps the same thing would have to be expressed as an ordering constraint between pages,
which is precisely what {doc}`pages` exists to avoid. Steps put the dependency between *stages*,
where it is real, instead of between *pages*, where it would be a lie.

## What a step hands the next one

`FilterData` — what a pipe is given besides the page — is an **enum**, not a struct with two fields:

| Variant | Available |
|---|---|
| at the **first** step | the target companies, and only those |
| at **later** steps | the accumulated results of all preceding steps, and only those |

The two are never available at once. An enum says so; a struct with two optional fields would
suggest they might both be there, and every pipe would have to handle a combination that cannot
occur.

```{note}
**Accepted consequence, stated rather than hidden**: a pipe that needs the *target companies* works
only when scheduled in the first step. This is a real limit and it is recorded in {doc}`limits`
rather than papered over — the alternative was carrying both kinds of filter data everywhere, which
would make every pipe pay for a case almost none of them want.
```

## Order is insertion order

Steps preserve the order they were declared in and deduplicate. Iterating them in hash order would
make the order of processing — and therefore the order of the output — unpredictable, and
reproducible tests impossible. See {doc}`determinism`.

## A class in two steps accumulates

If a page class appears in two steps, its pages are processed **twice** and the results **add up**
rather than the later step overwriting the earlier one.

In the ordinary case — one class, one step — the two behaviours are indistinguishable. They differ
exactly where overwriting would discard data that a step had already produced *and already fed
forward* to the step doing the overwriting. Accumulating is the only choice that cannot lose work
that something else already depended on.
