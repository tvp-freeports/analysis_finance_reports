"""What does the report print under or beside its management company label?

Most reports name their management company in the directory pages at the front, as a label
(``Management Company``, ``Società di gestione``, ``Sociedad gestora``, ``Gestora:``) with the name
on the line below or to the right. This probe finds the first such label and prints what surrounds
it, with fonts, so the anchor and the value can be checked against the page -- and against the other
reports of the same format, which is where an anchor has to hold.

A value may carry a period of office, ``(until 11 June 2024)`` or ``(from 12 June 2024)``, on its own
line or inside the same one: a report can name two companies in one year. Such lines are marked, so
that the most recent company is taken rather than simply the first one printed.

Usage:
    freeports-dev probe run manco [--max-pages N] PDF-OR-DIR...
    python probe_manco.py [--max-pages N] PDF...

Known limits: only the first label found is shown, and the first page carrying it is often the table
of contents (a line ending with a page number). Only the first 45 pages are read unless
``--max-pages`` says otherwise. A company named only inside prose is not found.
"""

import re
import sys

import pymupdf
from freeports.utils.pdf_extract import pdflines_from_pagedict

LABELS = (
    "management company",
    "società di gestione",
    "societa' di gestione",
    "società di gestione del risparmio",
    "sociedad gestora",
    "gestora",
    "société de gestion",
)
PERIOD = re.compile(
    r"\((until|since|from|fino al|dal|hasta|desde)\b[^)]*\)", re.IGNORECASE
)
TOC_ENTRY = re.compile(r"\s\d+\s*$")
DEFAULT_MAX_PAGES = 45


def is_label(text):
    low = text.strip().lower().rstrip(":").strip()
    return (
        any(low == label or low.startswith(label + " ") for label in LABELS)
        and len(low) < 60
    )


def below(lines, anchor, count=10):
    ax0, _, _, ay1 = anchor.bbox
    out = [
        ln
        for ln in lines
        if ln is not anchor
        and ln.text.strip()
        and ln.bbox[1] > ay1 - 1
        and abs(ln.bbox[0] - ax0) < 40
    ]
    return sorted(out, key=lambda ln: ln.bbox[1])[:count]


def beside(lines, anchor):
    _, ay0, ax1, _ = anchor.bbox
    out = [
        ln
        for ln in lines
        if ln is not anchor
        and ln.text.strip()
        and ln.bbox[0] > ax1
        and abs(ln.bbox[1] - ay0) < 3
    ]
    return sorted(out, key=lambda ln: ln.bbox[0])[:2]


def describe(line):
    mark = "   <- period of office" if PERIOD.search(line.text) else ""
    return f"{line.text.strip()!r}  [{line.font_name} {line.font_size:g}]{mark}"


def main(argv):
    max_pages = DEFAULT_MAX_PAGES
    if argv[:1] == ["--max-pages"]:
        max_pages, argv = int(argv[1]), argv[2:]
    if not argv:
        print(__doc__)
        return 2
    for path in argv:
        print(f"=== {path}")
        with pymupdf.open(path) as document:
            found = False
            for index in range(min(len(document), max_pages)):
                lines = pdflines_from_pagedict(document[index].get_text("dict"))
                for line in lines:
                    if not is_label(line.text):
                        continue
                    toc = (
                        "   (looks like a table of contents entry)"
                        if TOC_ENTRY.search(line.text)
                        else ""
                    )
                    print(f"  p{index + 1} label {describe(line)}{toc}")
                    for value in beside(lines, line):
                        print(f"      beside -> {describe(value)}")
                    for value in below(lines, line):
                        print(f"      below  -> {describe(value)}")
                    found = True
                    break
                if found:
                    break
            if not found:
                print(
                    f"  -  no management company label in the first {max_pages} pages"
                )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
