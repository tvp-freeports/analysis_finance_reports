# Target lists, and their provenance
A run does not search for the whole database; it searches for **lists**, named with `--target-list`
/ `-T`. A list is a curated set of companies with a provenance:

```text
Name,Institution,Date
TEST,FREEPORTS,2025-01-01
```

```text
List name,Company name
TEST,AP Møller Mærsk
TEST,Airbnb
```

The institution and the date are not decoration. A list is normally somebody's published position —
an exclusion list, a sector definition, a screening — and the output of a run is only as
interpretable as the answer to "whose list, as of when". Recording it in the database means the
answer travels with the data.

Several lists can be given at once; their companies are unioned.

## Two properties that will surprise you if nobody says them

**File order is significant and is preserved, never sorted.** Matching is *first match wins*, so a
shorter name that is a prefix of a longer one must sit **after** the more specific one — exactly
where you put it. Sorting the file alphabetically would silently reattribute holdings to the wrong
company, which is the kind of error that produces a plausible number rather than an obvious failure.

**Patterns are matched unanchored.** They search the whole string. This is what the real matcher
does, and validation deliberately does the same, so a pattern that is accepted is a pattern that can
fire. `\bmaersk` against `ap moller maersk` is the case that decides it: anchoring the validation
would reject a pattern that works perfectly.
