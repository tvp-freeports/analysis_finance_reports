# The document classification file

`metadata/document_classify.yaml` lets a formats repository recognise a document's format from the
document itself — from a line of its cover, a typeface on its title page — rather than from the URL
it was downloaded from. `freeports-dev init-format-repo` creates it with no rules; a file that is
absent, empty or only comments classifies nothing.

The grammar follows the format names ({doc}`names`): a name is a path down a tree, and the file is
written as that tree. This page is the grammar. When the author needs logic the grammar cannot
express, the Python module of {doc}`document-classify-python` runs first and hands over to this file.

## The idea in one example

```yaml
1:
  AMUNDI-IT24: {text: '^FONDI AMUNDI '}
  KAIROS-EN23: {text: '^KAIROS INTERNATIONAL SICAV'}
  MEDIOLANUM:
    any_of: [{text: '^Mediolanum'}, {text: '^MEDIOLANUM'}]
    IT24.*:
      A: {text: '^Relazione ', font: gillsansmt}
      B: {text: '^Relazione ', font: helvetica}
```

Read it top to bottom. On page 1: if a line begins `FONDI AMUNDI `, the document is `AMUNDI-IT24`
and the search ends. Otherwise, if a line begins `KAIROS INTERNATIONAL SICAV`, it is `KAIROS-EN23`.
Otherwise, if a line begins `Mediolanum` or `MEDIOLANUM`, it is *some* `MEDIOLANUM` format — not yet
which — and the search goes on inside that block, where the typeface of the title tells `.A` from
`.B`.

Three ideas carry the whole grammar:

- **Pages, then steps, both in the order written.** The first step that settles a complete name
  ends the search.
- **A step may settle only part of a name.** That partial result is kept, and later steps can only
  refine it.
- **Nesting is context.** A step inside `MEDIOLANUM` is looked at only once the document is known to
  be a `MEDIOLANUM` one.

## Pages

The top level lists the pages to look at, each with its steps. A page is an integer: `1` is the
first page, `-1` the last, `-2` the one before it; there is no page `0`.

Pages are tried **in the order written**, not in numerical order, and the search stops at the first
page that settles a complete name. That order is a statement about cost. Turning a page into lines
is the expensive part of classifying, so a file that puts page 1 first, and puts a later page only
where page 1 is not enough, usually reads a single page.

- A page the document does not have — page 5 of a four-page document — gives nothing for that entry,
  and the search goes on. It is not an error.
- The same page may appear more than once. It is read once per document, and "the same" means the
  same physical page: on a ten-page document `1` and `-10` are read once between them.
- Pages `1` and `-1` are the only ones every document has. That matters to the load-time checks
  ([below](#what-is-refused-when-the-file-is-loaded)).

## Steps and their criteria

Under a page comes a sequence of **steps**. A step is a key — a format name, complete or partial —
and a body: the **criteria** a page must meet for the step to match, and possibly **child steps**.

**A criterion is a line selection, written exactly as in the semistructured level's YAML** — the
mapping form of {doc}`../../guides/formats/selections`, with the same keys (`text`, `font`,
`font_size`, `area`, `window`, `font_of`, `any_of`, `all_of`, `without`). A criterion is met when it
selects at least one line of the page. Everything that page says holds here, in particular that text
anchors are verbatim strings with `^` and `$`, not regular expressions, and that sizes are best
written as ranges.

What document classification adds to selections is only this:

- A step's criteria are an **AND**: every one must select a line. The order among them does not
  matter. Two criteria of one step are written as a list, or as one mapping using `all_of`.
- An **OR** is written with `any_of` inside one criterion, or by repeating the step further down —
  in which case the order of the two copies matters like the order of any two steps.
- A criterion that states nothing — `{}`, `{any_of: []}`, a `without` on its own — is an error. It
  would select every line, and "the page has some text" is not a fact about a format.

### What a step does when it matches

| The step's name is | When its criteria match |
|---|---|
| **complete** (`KAIROS-EN23`) | the document is that format, and the search ends |
| **partial** (`MEDIOLANUM`, `FINECO-EN23@*`) | the partial name is kept as the **result so far**, and the search goes on — into the step's children, then to the steps after it, then to the next pages — looking for a more complete match |

The result so far is carried from one page to the next. The cover can say the entity, and the table
of contents the variant:

```yaml
- 1:
    ASTERIA: {text: '^Asteria Funds$', font: centurygothic-bold}
- 2:
    ASTERIA:                               # the year is told by page 2's header
      EN24: {text: '^Asteria Funds$', font: centurygothic-bold}
      EN23: {text: '^Asteria Funds$', font: centurygothic}
```

If the search ends on a partial name — nothing more complete was found — that partial name is the
outcome, and the formats of the repository it still allows are the candidates to try.

### Which steps are looked at

Once a partial result is held, a step is looked at only if its name **refines** it: once the document
is known to be `MEDIOLANUM`, a step for `AMUNDI` is skipped, and so is a second `MEDIOLANUM` step,
which would add nothing. The result can only grow more precise; it is never replaced.

That has a consequence worth writing down before writing the file: **a partial match is sticky.** If
a document of another group mentions `Mediolanum` on its cover — as a distributor, say — then from
that moment no step of any other entity is looked at, on any page. Two habits contain it:

- write the criteria of a partial step with the care of a complete one — anchored, specific — because
  in effect they decide the entity;
- put the partial steps of a page **after** the complete ones, so that a document a complete step
  recognises outright never reaches them.

## Nesting

A step's body may hold child steps after (or among) its criteria. A child's key is not a whole name
but a **suffix** that continues its parent's name from the first segment the parent leaves open:

| The parent ends with | A child key is | Example |
|---|---|---|
| the entity | language, or language and year, optionally with place and variant | `MEDIOLANUM` → `EN`, `IT23`, `ES24.*` |
| the language | the year, optionally with place and variant | `MEDIOLANUM-EN` → `23` |
| `@*` | the place, with or without its `@` | `FINECO-EN23@*` → `LUX` or `"@LUX"` |
| `.*` | the variant, with or without its `.` | `MEDIOLANUM-IT24.*` → `A` or `.A` |
| `@*.*` | place or variant **with** its separator, since a bare `LUX` could be either | `X-EN23@*.*` → `"@LUX"`, `.A` |
| the year | nothing: a complete name has no children | `FINECO-EN23` cannot have children; write `FINECO-EN23@*` |

A child must name a strictly more precise set than its parent, and it may itself end in `@*` or `.*`
and have children of its own.

**A nested block is context, not a shortcut.** A child is looked at only once its block has been
entered — once the result so far is at least as precise as the nearest enclosing step that has
criteria of its own. Where no enclosing step has criteria, the top-level key plays that role. So a
generic criterion such as `text: Relazione` under `MEDIOLANUM` can never fire on an Amundi report,
and a child with criteria is context for its own children in turn. A key with no criteria, which
only spells part of the name, filters nothing: adding or removing one changes no outcome.

A step whose criteria fail closes its whole block for that page.

## The unconditional step: `~`

A step with an empty body — `~`, `[]` or `{}` — has no criteria and always matches. It is the
**otherwise** of a block:

```yaml
1:
  ASTERIA:
    text: '^Asteria Funds$'
    EN24: {text: '^Investment Managers$'}
    EN23: ~                         # an ASTERIA document that is not EN24
```

A format decided this way was decided **by exclusion**, and the engine says so when it reports the
result. The price is that the criteria of the enclosing step now decide the format on their own for
every document on which `EN24` does not match — choose them with that in mind.

An unconditional step with a **complete** name at the top level decides every document still
unclassified when it is reached. Every later step that could only fire on an unclassified document
is then dead, and the load-time check reports each of them
([below](#what-is-refused-when-the-file-is-loaded)).

## The catch-all: `*`

A top-level step whose entity is `*` completes whatever entity has already been found:

```yaml
1:
  MEDIOLANUM: {text: '^Mediolanum'}
  "*-EN23": {text: '^Annual Report'}
```

`*-EN23` reads "*the entity already found, in English, 2023*". With `MEDIOLANUM` held it stands for
`MEDIOLANUM-EN23`, and its criteria are checked only if the repository has that format. If the
repository has no `EN23` for the entity found — or no entity has been found — the step simply does
not fire, and the result so far stands. That is not an error, at load time or at run time.

A catch-all must write at least the language, appears only at the top level, and has no children.
It is an error only when **no entity of the repository** could complete it — `"*-DE23"` in a
repository without a single `…-DE23`, or the slip `"*-EN32"` — because then it cannot be anything but
a typo.

## The verbose form and its shortcuts

Everything above is written in the short form. Underneath it there is one **verbose** form, in which
every level is a list:

```yaml
- 1:                                # a list of entries: one page each
    - EURIZON-EN23:                 # a list of steps
        - {font: arialmt-bold, font_size: [59, 61]}   # criteria and children, in a list
    - MEDIOLANUM:
        - {text: '^Mediolanum'}
        - EN: [{text: '^Annual Report'}]
```

The verbose form can always be written; each shortcut may be taken when it is unambiguous.

| Shortcut | Allowed when | Read as |
|---|---|---|
| pages as a mapping, `{1: …, 2: …}` | no page is repeated | the list of entries, in the same order |
| steps as a mapping, `{AMUNDI-IT24: …, KAIROS-EN23: …}` | no step is repeated | the list of steps, in the same order |
| a body as one mapping mixing selection keys and child keys | always | the selection keys form one criterion; the other keys are the children, in the order written |
| a body as a single criterion | the step has one criterion | a list of that one criterion |
| `~`, `[]` or `{}` as a body | — | no criteria, no children: the unconditional step |

Whether a key is a criterion or a child is decided by the key: selection keys are lower-case words
from the fixed list above, names are not. The one place the two could meet is a variant named like a
selection key (`text`, `area`) under `.*`; write it with its separator, `.text`, to make it a child.

Repeating a step — the OR of two copies of one step — needs the list form, since a mapping cannot
hold the same key twice. "The same key" is judged on the names the keys stand for: `21` and `"21"`,
`LUX` and `"@LUX"` under `@*`, `A` and `.A` under `.*` are duplicates.

### What the YAML reader does to keys

The file is read as YAML 1.2, and three of its rules meet the grammar:

- **`*` and `@` cannot begin an unquoted key**: `*` starts an alias, `@` is reserved. Write
  `"*-EN23":` and `"@LUX":`.
- **Numbers are read as numbers.** `23:` is an integer and is accepted as a year (`5:` becomes
  `05`) or as a variant (`1:` becomes `.1`). But `24.10:` is the number `24.1`, and `true:` or `null:`
  are not text: such keys are refused with the advice to quote them, because the spelling is already
  lost. `08:` with a leading zero stays text.
- **The order of a mapping is kept.** That is what lets a mapping stand in for a list.

## What is refused when the file is loaded

The file is checked against `metadata/formats.csv` as soon as the repository is loaded, before any
document is opened. The guarantee is that **no complete result of the file can be a name the
repository does not declare**, and that no step was written that no document could ever reach.

Every message names the file and the place: the page, the enclosing steps by their full names, and
the key —

```text
…/metadata/document_classify.yaml: page 1, step MEDIOLANUM, key '-ES': a child key is a suffix without the hyphen: write 'ES'
```

A file with several defects reports all of them, one per line, in file order, so one correction
round fixes them all.

### Errors of form

| Message | Caused by |
|---|---|
| `a page is an integer, 1-based, negative counted from the end` | a page key that is not an integer, including `"1"` |
| `there is no page 0: pages are 1-based, negative counted from the end` | page `0` |
| `this page has no steps, so it cannot classify anything` | `1: ~`, `1: []` |
| `an element of the list of entries is one page: write several pages as several elements` | `- {1: …, 2: …}` in the verbose form |
| `an element of a list is one step: write several steps as several elements` | one list element holding two step keys |
| `an element of a list is either a criterion or one step, not both` | one list element mixing selection keys and a step key |
| `duplicate key: two keys of this mapping name the same step; write the steps as a list to repeat one` | a repeated page or step in a mapping, including `21`/`"21"` |
| `not a format pattern: …` | a top-level key that does not read as a name ({doc}`names`) |
| `a child key is a suffix without the hyphen: write 'ES'` | `-ES` under `MEDIOLANUM` |
| `this key does not strictly refine its parent '…': …` | a child that fills a segment the parent closed, or adds nothing |
| `'FINECO-EN23' is a complete name and cannot have children: write 'FINECO-EN23@*'` | children under a complete name |
| `below '@*.*' a key needs its separator: '@' for a location, '.' for a variant: write '@LUX' or '.LUX'` | a bare child key under `@*.*` |
| `this key is not read as text by YAML (a float, a boolean or a null): quote it` | `24.10:`, `true:`, `null:` as a child key |
| `a '*' template is allowed only as a top-level step` / `a '*' template cannot have children` | a misplaced catch-all |
| `this criterion states nothing, so it would select every line of the page` | `{}` and its equivalents as a criterion |
| `invalid line selection: …` | a criterion the selection grammar refuses |

When YAML itself refuses the text before the grammar is reached, its message is passed on, with a
hint for the two common cases: `an unquoted '*' opens a YAML alias: quote a template key, as in
"*-EN23":` and `'@' cannot open an unquoted YAML value: quote a location key, as in "@LUX":`.

### Errors against the repository

| Message | Caused by |
|---|---|
| `'MEDIOLANUM-IT24' is not a format of formats.csv` | a complete name the repository does not declare — including one it has only with variants, whose set is written `MEDIOLANUM-IT24.*` |
| `no format of formats.csv refines the partial '…'` | a partial name no format falls under |
| `no entity of formats.csv completes the template '*-DE23'` | a catch-all nothing could complete |
| `no document can reach this step` | a step that, given what can be held when it is visited, is never looked at — usually a step written after one that always pre-empts it, or a block with no criteria of its own whose entity nothing before it can find |
| `no document can reach this step: the unconditional step '…' on page 1 decides every document that gets there first` | the same, caused by an unconditional complete step before it |

The reachability check never names a step that some document could reach. It is cautious in the
other direction: a step reachable only on documents long enough to have a given page is counted as
reachable. That is also why an unconditional step closes off what follows it only on pages `1` and
`-1`, which every document has. When the repository has a Python module, the check assumes the module
could hand over any partial name of the repository, so that a step meant to refine what the module
found is not wrongly reported.

## A fuller example

Excerpts from the rules written for the formats of `analysis_finance_reports_formats`, which show the
grammar on real covers:

```yaml
- 1:
    # One format, one cover line. Anchored at the start only: these titles end in a space.
    CARNE-EN23:        {text: '^SIDERA FUNDS'}
    FINECO-EN23@IR:
      any_of: [{text: '^FAM Series UCITS ICAV'}, {text: '^FAM Evolution ICAV'}]
    FINECO-EN23@LUX:   {text: '^CoRe Series'}
    UBS-EN23:          {font: ubsheadline}        # the title typeface, where the words are shared
    EURIZON-IT24:      {text: '^Relazione ', font: trebuchetms, font_size: [25.5, 26.5]}

    # Partial roots after the leaves: a partial match is sticky.
    EURIZON:
      font: arialmt-bold
      any_of: [{text: '^Eurizon'}, {text: '^EURIZON'}, {text: '^Epsilon'}]
      EN21: {text: '^Eurizon AM Sicav'}
      EN23: {font: arialmt-bold, font_size: [59, 61]}
    MEDIOLANUM:
      any_of: [{text: '^Mediolanum'}, {text: '^MEDIOLANUM'}, {text: '^CHALLENGE FUNDS'}]
      EN:
        23: {text: '^Mediolanum Specialities SICAV-SIF'}
        24: {text: '^CHALLENGE FUNDS'}
      IT24.*:
        A: {text: '^Relazione ', font: gillsansmt}
        B: {text: '^Relazione ', font: helvetica}

# Image covers: the title is on page 4 or 5. Only reached when page 1 settles nothing.
- 4:
    EURIZON-IT24:      {text: '^Relazione ', font: trebuchetms, font_size: [25.5, 26.5]}
- 5:
    EURIZON-IT24:      {text: '^Relazione ', font: trebuchetms, font_size: [25.5, 26.5]}
```

Two things it shows that a first file tends to get wrong. `FINECO-EN23@IR` and `FINECO-EN23@LUX`
are two leaves rather than a `FINECO-EN23@*` block, because their covers share no text that could
serve as the block's own criterion — and a block without one is entered only once something has
already found `FINECO-EN23@*`, which here nothing would. And the same step appears on three pages
because some covers of that format are images: an entry that finds nothing costs one page read, an
entry left out costs a document that is never classified.
