# How formats are named

A format name such as `MEDIOLANUM-IT24.A` is a path down a tree. Each segment narrows the one
before, and the segments are ordered by how likely a change in them is to change the shape of the
PDF. This page is the prescription that names are expected to follow, the reasoning behind it, and —
first, because it is the question the whole convention exists to answer — how to use it when you
have a document and do not know which format reads it.

## I have a document: which format do I use?

The situation is common. The document's URL is not in the repository's `url_mapping.csv`, the
repository's own classification ({doc}`document-classify-yaml`) does not recognise it, and nobody
has told you its format. The names are built so that you can guess, and so that your guesses come
in a sensible order.

```{admonition} The heuristic, in one paragraph
:class: tip

Rebuild the name the document would have — **entity, language, year, and place of distribution if
it says one** — from what its first and last pages show. Then try the formats whose names agree
with it, from the most specific agreement to the least: every variant and every place under the
same entity, language and year first; then the same entity and language in other years, nearest
year below yours first; then outward. Stop at the first format whose output is right. If the
repository runs out, the document needs a new format, and the one that came closest is where it
starts.
```

Step by step:

1. **Rebuild the name.** Read off the four segments described in [the segments](#the-segments): the
   entity named at the start of the document, the language it is written in, its year of
   publication, and — only if the document addresses a particular country — that country.
2. **Same entity, language and year.** Try every format that differs from your rebuilt name only in
   the place or the variant: `FINECO-EN23@LUX` and `FINECO-EN23@IR`; `MEDIOLANUM-IT24.A` and
   `MEDIOLANUM-IT24.B`. The variant carries no meaning you could read off the document
   ([below](#the-segments)), so there is nothing to choose between variants except by trying them.
3. **Same entity and language, another year.** A name carries the **earliest** year its recipe reads
   ([the year rule](#one-name-per-recipe-and-the-smallest-year)), so a recipe named for an earlier
   year is the one that may already extend to yours. For a 2022 document, with `EURIZON-EN21` and
   `EURIZON-EN25` in the repository, try `EN21` first and `EN25` second. If `EN25` turns out to read
   2022, that is news worth reporting: the format should be renamed `EURIZON-EN22`.
4. **Further out.** The same entity in another language; then entities whose names sit next to it.
   The odds fall with each step, and at this distance a format that works is less a find than a
   sign that two recipes might be one — something the repository's maintainers will want to hear.
5. **Nothing works.** The document needs a format of its own, and it is named by the rules
   [below](#naming-a-new-format). The format that came closest in the search is the one to copy from.

When the repository's own classification gets part of the way — it knows the entity and the
language but cannot tell the variant — the engine's `classify` command prints that partial name
together with the formats of the repository it still allows. That list *is* step 2 or 3 of this
heuristic, already worked out for you.

## The segments

```text
[entity]-[language][year][@place][.variant]
```

Read as a tree, `[entity]/[language]/[year]/[@place]?/[.variant]?`, the way a file system would list
it. Entity, language and year are always present; place and variant are optional.

| Segment | Written | Means | Example |
|---|---|---|---|
| entity | any characters except `-`, `@`, `.`, `*` and spaces | the name the document is identified by — in practice the group behind the funds, as named at the start of the document | `MEDIOLANUM`, `ANIMA_SGR` |
| language | two capital letters | the language the document is written in | `EN`, `IT`, `ES` |
| year | two digits | the year of publication | `23`, `24` |
| place | `@` and two or three capital letters | the country the report is addressed to, when there is one | `@LUX`, `@IR` |
| variant | `.` and anything without `.` or `/` | nothing: it only tells apart two recipes no other segment separates | `.A`, `.B` |

A repository never writes these names out: `metadata/formats.csv` lists the five segments in
columns, and the name is synthesised from them ({doc}`../../guides/formats/repository`). So a name
and its parts cannot disagree.

### The order: what is most likely to change the PDF

The segments are ordered by the **supposed probability that a change in that segment changes the
shape of the PDF**. That is a hypothesis about how reports are produced, stated as one; it was
chosen because it is plausible, not because it was measured.

| Segment | Why it sits there |
|---|---|
| entity | An issuer brings everything that shapes a document at once — its expertise and habits, the software that typesets its PDFs, the firm it outsources the work to, its internal policies. A different entity is the most likely source of a different layout, so it is the first cut. |
| language | Changing language changes every label, which matters to any recipe that reads labels. In principle it need not change the structure, so it comes after the entity. |
| year | Everything the entity contributes, and the regulation it answers to, drifts over time. Over several years that drift reaches the structure. |
| place | A report produced for a different audience may differ in detail or emphasis, rarely in shape. |
| variant | Not a property of the document at all. It is there because there is exactly one recipe per name, and some recipes differ for technical reasons nothing on the cover reveals. |

### The three criteria the names answer to

The segments were chosen by asking three things of a naming scheme:

1. **Names that are close must mean recipes that are close.** Sorted alphabetically, as a file
   listing sorts them, similar documents should end up next to each other and different ones far
   apart.
2. **The rules for giving a name must be simple and unambiguous**, so that two people naming the
   same document arrive at the same name.
3. **What the name is made of must be evident in the document** — on its first or last pages,
   present, and not open to interpretation. A segment that could only be filled by reading the
   whole report would make the name useless for the question at the top of this page.

The first criterion is what gives the names their use beyond identification. If names are given
well, two documents that differ only in their last segment are cousins, and two that differ in their
entity are strangers. That gives the work on recipes a **locality**: when trying to merge two
recipes, try neighbours first and distant names later; when one recipe has to be split, the split
falls at the segment where the documents actually diverge. The name becomes an account of *why* two
documents look alike or do not.

The third criterion is where the convention is weakest in practice. The entity is meant to be the
evident name at the start, but the name a reader's colleagues use for the group is not always the
largest word on the cover: in the repository, a cover reading `WILLERFUNDS` is a `FIDEURAM` format,
`CHALLENGE FUNDS` a `MEDIOLANUM` one, `SIDERA FUNDS` a `CARNE` one. When rebuilding a name, if the
cover's own name finds nothing, the name of the management company printed further in is the next
thing to try.

## One name per recipe, and the smallest year

**Names follow the recipe, not the document.** There is exactly one recipe per name, and a document
is filed under the name of the recipe that reads it — whatever its own year or entity says. A 2022
document read by `EURIZON-EN21` is an `EURIZON-EN21` document; it does not earn a new name for
being from 2022.

Two rules settle which name a recipe carries when its documents disagree:

| When a recipe reads documents… | It is named for | Why |
|---|---|---|
| of several years | the **earliest** of them | The search above tries the nearest earlier year first. A recipe named for its first year is found by everyone whose document is from that year or later. |
| of several entities | the **shortest** entity name; at equal length, the **most widely known** | To maximise the chance that the name coincides with what the user reads on their own document. |

It follows that a rename is a normal event. When a recipe named `EN25` is found to read a 2022
document too, it becomes `EN22`.

## Naming a new format

When a document needs a recipe of its own, rebuild its name as in the first step of the heuristic,
then:

1. If no format of the repository has the same **entity, language and year**, that is the name. No
   place, no variant.
2. If one does, look at whether the two documents are addressed to **different places**. If they
   are, the place is what separates them: `FINECO-EN23@LUX` and `FINECO-EN23@IR`.
3. Otherwise nothing evident separates them, and the difference goes into the **variant**:
   `MEDIOLANUM-IT24.A` and `MEDIOLANUM-IT24.B`. In the repository, where a variant has been needed,
   every format of that entity, language and year carries one, so a bare name never sits beside its
   own variants.

The variant is the last resort on purpose. It is the only segment a user cannot read off a
document, so every variant added is one more format to try blindly at step 2 of the heuristic.

## Partial names

The same grammar, cut short, names a **set** of formats rather than one. A partial name is how the
repository's classification ({doc}`document-classify-yaml`, {doc}`document-classify-python`) and
`url_mapping.csv` say *"this much is certain"* when they cannot say everything.

| Written | Stands for | Example |
|---|---|---|
| entity alone | every format of that entity | `MEDIOLANUM` |
| entity and language | every year, place and variant under them | `MEDIOLANUM-IT` |
| ending in `@*` | the formats that **have** a place, any place | `FINECO-EN23@*` |
| ending in `.*` | the formats that **have** a variant, any variant | `MEDIOLANUM-IT24.*` |
| ending in `@*.*` | both open | `X-EN23@*.*` |

Three rules make the reading unambiguous:

- **A name is cut only from the right.** `MEDIOLANUM-IT` is a partial name; `-IT24` and
  `MEDIOLANUM-24` are not names at all.
- **Once the year is written, a missing place or variant means *none*.** `MEDIOLANUM-IT24` is a
  complete name, and it means the format with no place and no variant. In a repository that has only
  `.A` and `.B`, that name does not exist, and using it is an error; the set of both is written
  `MEDIOLANUM-IT24.*`. The `@*` and `.*` endings exist so that being partial is always said, never
  implied — which is also what lets a typo in a year be caught instead of quietly becoming a set.
- **`*` is reserved.** No format may contain it, so a partial name can never be mistaken for a
  format.

A partial name is only useful if some format of the repository falls under it, and the engine
checks that wherever one is written. An entity containing `-` is legal in a complete name but
cannot be written in a partial one, because the cut would be ambiguous; none of the repository's
formats has one.
