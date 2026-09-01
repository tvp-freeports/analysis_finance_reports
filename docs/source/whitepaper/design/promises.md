# Promises

*Implemented.* The answer to the one thing {doc}`pages` cannot handle: a value the page needs but
does not carry.

## The idea

A value a page cannot resolve on its own becomes a **promise** — a named placeholder — and the run
continues. Nothing waits, nothing is re-read, and the page stays the unit of work.

```text
page 412 produces:  Investment { fund: Promise("fund-name-of-section-3"), … }
page 3   deposits:  ("fund-name-of-section-3", "Global Equity Fund")
                                   │
                         at the end of the run
                                   ▼
                    Investment { fund: "Global Equity Fund", … }
```

Neither page knows about the other. Page 3 does not know anyone wants that name; page 412 does not
know where it will come from. They agree only on an **id**.

## Three stages

**1. Collection.** As pages are deserialized, every pipe deposits `(id, value)` pairs into a
**multimap**. A multimap, not a map, because several pages legitimately contribute to the same id: a
fund name printed at the head of every page of its section, a total repeated at the foot of each
table. The first contribution is not privileged and the last does not overwrite.

**2. Flattening.** When the document is finished, references between promises are followed and
replaced, and every id keeps its own *sequence* of contributions.

**3. Resolution.** Entities with pending fields are resolved against the flattened map.

## Three properties that are not obvious

These are the parts that bite people writing a pipe, and each was decided against a plausible
alternative.

### A container is not a contribution

```text
[{"id": a}, {"id": b}]     two contributions
{"id": [a, b]}             one contribution that happens to be a list
```

Flattening synthesises no lists, so the two stay distinguishable, and a pipe wanting to deposit two
values must return two dicts.

This distinction is the reason the module exists in its current form. An earlier design conflated
them, and the conflation is **unrecoverable**: once you cannot tell "two values" from "one list", no
later stage can work it out. It matters exactly when the value being promised *is itself* a list —
rare, and silently wrong when it happens, which is the worst combination.

Changing it cost authors nothing, which was verified before the change: a pipe returning a list of
dicts was already flattened, so the capability existed and only the ambiguity was removed.

### A null is a non-contribution

Discarded during flattening as if never deposited. An id whose contributions were **all** null
vanishes exactly like an id that never had any — there is no third state of "present but empty" for
a later stage to interpret differently from the first two.

### A pending reference is not an error where it is noticed

A promise pointing at an unknown id **survives flattening**. The policy is decided later, at
resolution: a non-strict promise makes its entity disappear, a strict one raises an error.

The reason is a diagnostic one. If flattening failed on an unresolved reference, "circular" would
become the message people see for every missing value, and would stop meaning anything. Deferring it
keeps a **circular error always a real cycle**, never a missing-value report wearing the wrong name.

## Resolution has three outcomes, each with a name

Not an `Option<Vec<_>>` for the caller to interpret:

| Outcome | Means |
|---|---|
| resolved | the entity's pending fields are filled in place |
| dropped | a non-strict promise had nothing to resolve to; the entity disappears |
| **multiplied** | a field marked *multiple* expands its entity into one copy per value |

Multiplication is a cartesian product when more than one field is multiple. Ordinary fields resolve
**first** and multiple fields expand **second**, so the copies are made from an already-resolved
entity rather than resolving the same fields once per copy — the same answer, computed once.

## The maps are ordered

Insertion or key order, never hash order. For equal content the cycle a failure reports is always
**the same** cycle, so an error message can be asserted in a test. See {doc}`determinism`.
