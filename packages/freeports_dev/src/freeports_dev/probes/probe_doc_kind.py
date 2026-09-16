"""What kind of document is this, and does it carry the SFDR periodic disclosure?

Two reports filed under one format name are not always the same kind of document. An annual report
and a semi-annual one from the same issuer share the layout but not the sections: the SFDR periodic
disclosure and its sustainability indicators are usually only in the annual one. A datum that is
present in some reports of a format and absent in others is, more often than not, a datum that
belongs to one kind of document -- look at the cover before concluding the format is not uniform.

For each document it prints the number of pages, the first page carrying the European SFDR template
(or ``-``), and the first line of text on the cover, which in financial reports nearly always says
which report it is.

Usage:
    freeports-dev probe run doc_kind PDF-OR-DIR...
    python probe_doc_kind.py PDF...

Known limits: when the cover is a logo drawn as vector graphics, its "first line" is whatever text
comes first, or nothing. The template is recognised by its heading or by its first question, in
English, Italian, Spanish and French; a disclosure written outside the template is not found.
"""

import re
import sys

import pymupdf
from freeports.utils.pdf_extract import pdflines_from_pagedict

TEMPLATE = re.compile(
    r"(template periodic disclosure|periodic disclosure for the financial products"
    r"|did this financial product have a sustainable\s+investment objective"
    r"|modello di informativa periodica|informativa periodica per i prodotti finanziari"
    r"|questo prodotto finanziario aveva un obiettivo di investimento sostenibile"
    r"|plantilla de (divulgaci[oó]n|informaci[oó]n) peri[oó]dica"
    r"|informaci[oó]n peri[oó]dica de los productos financieros"
    r"|mod[eè]le d.informations p[eé]riodiques)",
    re.IGNORECASE,
)


def cover_line(document):
    lines = pdflines_from_pagedict(document[0].get_text("dict"))
    for line in sorted(lines, key=lambda ln: (ln.bbox[1], ln.bbox[0])):
        if line.text.strip():
            return line.text.strip()
    return "(no text on the cover)"


def first_template_page(document):
    for index in range(len(document)):
        if TEMPLATE.search(document[index].get_text("text")):
            return index + 1
    return None


def main(paths):
    if not paths:
        print(__doc__)
        return 2
    print(f"{'pages':>5}  {'SFDR':>5}  cover")
    for path in paths:
        with pymupdf.open(path) as document:
            page = first_template_page(document)
            print(
                f"{len(document):5d}  {page if page else '-':>5}  {cover_line(document)[:70]}  [{path}]"
            )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
