# Finding the pages, and reading one

The first step is not writing a selection. It is finding out **how many pages carry the datum**,
and then reading one of them properly.

## 1. Find them

`find-text` is the only command that works before the format knows anything, because it searches
the document rather than the format's idea of it:

```console
$ freeports-dev find-text -f BLACKROCK-EN25 --regex 'renam|changed its name' --max-hits 20
tests/formats/BLACKROCK-EN25/report.pdf:5  y=  508.7 x=  60.8- 565.6  arialmt[8.5]  iShares ESG Emerging Markets Government Bond Index Fund (IE) changed its name to iShares Emerging Ma…
```

`--text` is a substring and `--regex` is a regular expression, and they are separate options
because a {doc}`selection <../selections>` can never be a regular expression. Searching with one
and then carrying the pattern into a selection is the usual first mistake.

`--pages-only` prints `document:page`, which is what `inspect-page` takes.

## 2. Read the page as the engine reads it

```console
$ freeports-dev inspect-page -f BLACKROCK-EN25 -p 106 -m lines --order x --columns 2
```

`--mode lines` prints every line of the page with its font, its size and its box, preceded by a
legend naming each distinct *(font, size)* pair once. It does **not** load the format, so it works
on a format that does not exist yet.

`--order x --columns 2` is worth typing by reflex. Many pages of these reports carry **two tables
side by side**, and the default top-to-bottom order interleaves them into something unreadable.

What you are looking for is the answer to one question: **what distinguishes the thing you want
from everything else on the page?** On a holdings table it is usually the face. Here is one, and
the whole configuration that reads it follows from what this picture shows:

```{image} ../assets/holdings-rows.png
:alt: A crop of a holdings table: a bold country heading, five lines of a wrapped instrument name, and the figures level with its last line.
:width: 100%
```

Three things, all of them decisions:

- **the body is `arialmt 8.5` and everything that is not body is `arial-boldmt 8.5`** — the country
  heading `Germany (continued)`, the column headings, the totals. A selection on the face alone
  therefore excludes the headings, which is what keeps a format out of the commonest trap of the
  `Collapse` cells ({doc}`parameters`);
- **the instrument's name wraps over five lines**, and the figures sit level with its **last** one.
  That is one cell, and which direction puts it back together is read off this picture, not
  guessed;
- **the superscript `1`** after `02/06/2035` is a footnote mark. In the engine's line model it is a
  line of its own at 4.96 points, so a body selection at 8.5 leaves it out without being told to.

## 3. Count the pages the anchor holds on

This is the step that is easy to skip and expensive to skip.

A classification anchor read off one page of `BLACKROCK-EN25` was `Currency Holdings`, the column
heading, which that page writes as one text object. Measured over the document it covered **870**
pages. The heading `SCHEDULE OF INVESTMENTS` covered **1 063**. The 193-page gap was not pages
without a table: on **113** of them the same generator emitted `Currency` and `Holdings` as **two**
text objects, same table, same columns, same everything.

```{image} ../assets/holdings-header.png
:alt: A column heading reading "Currency Holdings" as a single run of text.
:width: 90%
```

```{image} ../assets/holdings-header-split.png
:alt: The same heading on another page of the same document, with "Currency" and "Holdings" set as two separate text objects.
:width: 90%
```

The anchor that holds is `Investment`, the third column's heading: **983 pages of 983**. The
eighty pages of the schedule it leaves out are a different table — forward currency contracts, and
the per-sub-fund summaries — and that was measured too, by searching those eighty pages for every
company the input database names and finding only the umbrella's own name in the running head.

**Declaring an absence means saying what you searched for.** "There is nothing there" is not a
measurement; "the seventy-six patterns of the target list match nothing on those eighty pages
except the running head" is.

## 4. Then verify what varies

An anchor that holds on every page of one document is still only one document. The property that
makes a format worth writing is **constancy inside the format**, so the next measurement is the
same anchor over the *other* reports of the same format.

Where the format has only one report — a new issuer, a first year — that measurement cannot be
made, and the honest substitute is the one above: every page of every class, with the number
written down. Say which of the two you did. A rule found on a single page has been propagated
through a status file, a plan and three documents before anybody looked at the page after it.

## The trap in the modes that follow

`inspect-page -m pdf_blks` shows what the first segment made of the page and needs nothing. The
other two are different:

**`-m txt_blks` and `-m results` need the right filter data, and that is where an hour goes.** A
`text_filter` that drops everything it cannot tie to a fund of interest prints nothing when it is
fed the wrong one, and an empty printout looks exactly like a broken algorithm. The command reads,
in order, `--filter-data`, then the page type's own
`tests/formats/<FORMAT>/[<n>/]pages/<page type>/filter_data.json`, then the target list — and it
prints which one it used. If the line it prints is not the fixture you meant, pass `--filter-data`
rather than reading the empty output as a result. {doc}`recipes-and-tests` is about building one
that proves something.
