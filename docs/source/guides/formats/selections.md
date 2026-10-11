# Selections: saying where to look

Most of what a format author writes is a **selection** over the lines of a page: a predicate
combining font, size, position, and text anchors.

## What a selection is

A conjunction of conditions, any of which may be omitted:

| Condition | Says |
|---|---|
| font | the line is set in this face |
| font size | exactly, or within a range |
| area | the line falls within these coordinates |
| text anchor | the line's text matches this, with `^` and `$` as anchors |

```python
from freeports.utils.pdf_extract import PdfLineSelection

body_set = PdfLineSelection(font="ArialMT", font_size=(6.95, 6.97))
```

The size range is not fussiness: a PDF's nominal 7-point body text is rarely exactly 7.0, and a
range is how you say "the body face" rather than "this one line". The same holds across documents
of one format: a title set at 26.00 points in one PDF is 26.03 in the next, and a single size is
matched within a thousandth of a point.

## Composing selections, and building one relative to another
Selections compose with `&`, which is ordinary conjunction. The useful part is that they can also be
built **relative to another selection**:

```python
PdfLineSelection.area_from_movewindow(target=header, vec=(0, 12), width_mult=1.0)
```

*"the region displaced from wherever **that** is"*. This is how a layout is described without
hard-coding coordinates that change on the next page — the header moves, and the window moves with
it.

Reaching for absolute coordinates is almost always the wrong first move. They work on the page you
are looking at and fail on the page after it, and the failure is a silently empty extraction rather
than an error.

## The two forms a configured selection takes

A format that writes no Python says the same things in configuration, and there are two spellings
of them. Which to use follows from the medium.

**In YAML — the semistructured level — write the mapping.** It is a key per condition, and it nests:

```yaml
label:
  font: [Lato, Open Sans, Lato-Regular]   # a union of faces: a list
  font_size: [6.9, 7.0]                   # a range; one exact size is a scalar
  text: '^VALORE COMPLESSIVO NETTO'
  area:
    x_max:                                # a side may be a nested selection: "as far as that line"
      font: arial-boldmt
      text: Legal adviser
  window:                                 # the box of the anchor's line, displaced and scaled
    anchor: {font: arialnarrow-bold, text: Management Company}
    dx: -0.1
    dy: 0.5
    width: 10.0
    height: 1.8
  without:                                # set difference, as a list because it folds
    - text: '^ $'
  any_of:                                 # set union
    - area: {x_max: 200, y_min: 785}
    - area: {x_min: 450, y_min: 785}
  all_of:                                 # set intersection, for two criteria of the same kind
    - area: {y_max: {text: SWAPS}}        #   closed by a heading where there is one…
    - area: {y_max: 750}                  #   …and by an ordinate where there is not
  font_of:                                # the face of another line, rather than a face named here
    text: n de la cartera
```

Every key is optional, and the whole mapping may be. A key the mapping does **not** know is an
error rather than a condition quietly dropped, which matters because a selection stating nothing
selects every line of the page.

**In a CSV cell — the structured level — write the compact form**, because a cell cannot hold a
mapping. It is also what `pdfline_selection_from_str` takes from Python:

```text
ArialMT[6.96](160:786)                    font, size, vertical band
Arial-BoldMT "^Annual report including"   font plus a text anchor
Lato|Open Sans[6.9:7.0] ((:370)(:))       a union of faces, a range, an area
TrebuchetMS-Bold "^TOTALE" / "CLASSE"      set difference — the spaces are required
window(<arial-bold "^Assets">, 1.2, -1, 100, 1.5)
```

A side left **empty** is unbounded; writing `0` there is an error, because a bound of zero and no
bound at all are different statements and only one of them is usually meant. A text anchor goes in
quotes, or it reads as a face name.

The mapping's own keys already intersect, so `all_of` is only for what they cannot reach: **two
criteria of the same kind**, such as two bounds on one coordinate.

The compact form expresses everything the mapping does except three keys with no compact spelling:
the two **set operations** `any_of` and `all_of`, and `font_of`. New expressiveness is added to the
mapping first.

`font_of` is the one criterion that cannot be written down, because it is not a property of the
lines being selected but of a line somewhere else on the same page: *the same face as that
heading*. A format reaches for it when it names no face at all — MEDIOLANUM-ES24.A's portfolio is
typeset in whatever family its column heading happens to use, and writing the family instead would
pin the format to one typesetting of one document. A page the anchor is not on selects **nothing**,
which is the right reading: a portfolio page without its heading is not a portfolio page.

## Anchors are not regular expressions

Text anchors are matched **verbatim**, with `^` and `$` meaning start and end — so prefix, suffix,
substring and exact are all expressible, and nothing else is.

The line is matched **as the PDF spells it**, without trimming. Many title lines end in a space —
`'SIDERA FUNDS '`, `'CHALLENGE FUNDS '` — and on those `'^SIDERA FUNDS$'` finds nothing. Unless
you have seen the exact line in `inspect-page`, anchor at the start only.

That is a deliberate restriction, not a missing feature. Regular expressions appear in
`text_filter`, where **meaning** is being read out of a block; a layout predicate does not need them
and is faster without. Keeping the two apart is the same separation the segments themselves are
built on: `pdf_extract` looks at graphical evidence, `text_filter` at meaning
({doc}`../../reference/design/segments`).

## Finding out which selection to write
The line-set modes of `inspect-page` exist for exactly this, and are the fastest way to learn what
font and size a value is actually set in:

```console
$ freeports-dev inspect-page -f CARNE-EN23 -p 25 -m structured --strings "Total Assets"
```

It searches the page for those strings and prints the lines that match, as that level's selection
machinery sees them. {doc}`dev-loop`.
