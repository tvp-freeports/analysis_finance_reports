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
range is how you say "the body face" rather than "this one line".

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

## The compact textual form

The same selections appear in the structured CSVs, where there is no Python to write them in, and
are available from Python through `pdfline_selection_from_str`:

```text
ArialMT[6.96](160:786)                    font, size, vertical band
Arial-BoldMT "^Annual report including"   font plus a text anchor
```

## Anchors are not regular expressions

Text anchors are matched **verbatim**, with `^` and `$` meaning start and end — so prefix, suffix,
substring and exact are all expressible, and nothing else is.

That is a deliberate restriction, not a missing feature. Regular expressions appear in
`text_filter`, where **meaning** is being read out of a block; a layout predicate does not need them
and is faster without. Keeping the two apart is the same separation the segments themselves are
built on: `pdf_extract` looks at graphical evidence, `text_filter` at meaning
({doc}`../design/segments`).

## Finding out which selection to write
The line-set modes of `inspect-page` exist for exactly this, and are the fastest way to learn what
font and size a value is actually set in:

```console
$ freeports-dev inspect-page -f CARNE-EN23 -p 25 -m structured --strings "Total Assets"
```

It searches the page for those strings and prints the lines that match, as that level's selection
machinery sees them. {doc}`dev-loop`.
