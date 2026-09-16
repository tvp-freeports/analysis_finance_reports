"""Where are the fund's total assets, liabilities and net assets, and do they add up?

A statement of net assets closes with three totals, and the accounting equation ties them:
``total assets - total liabilities = net assets``. This probe finds the first line starting with
each of the three labels of a language profile, takes the numbers printed to its right on the same
row, and checks the equation on the first column -- a probe that finds three numbers that add up has
almost certainly found the right three.

Profiles: ``en`` (``Total assets`` / ``Total liabilities`` / ``Total net assets``, and
``Net assets at the end of the``) and ``it`` (``TOTALE ATTIVITÀ`` / ``TOTALE PASSIVITÀ`` /
``VALORE COMPLESSIVO NETTO``). It also prints the first lines of the page, where the fund's name and
the date usually are.

Usage:
    freeports-dev probe run assets en|it PDF-OR-DIR...
    python probe_assets.py en|it PDF...

Known limits: only the **first** occurrence of each label is read. Many reports print a *combined*
statement of all sub-funds before the per-sub-fund ones, and the combined one is what this finds
first; the sub-fund statements come after it. A total whose numbers sit on the next line (a label
that wraps) yields no numbers. A liability written with a minus sign is compared by magnitude.
"""

import re
import sys

import pymupdf
from freeports.utils.pdf_extract import pdflines_from_pagedict

ROW_TOLERANCE = 2.0
NUMBER = re.compile(r"^\(?-?[\d.,\s ]{3,}\)?%?$")

PROFILES = {
    "en": {
        "assets": ("TOTAL ASSETS", "Total assets", "Total Assets"),
        "liabilities": ("TOTAL LIABILITIES", "Total liabilities", "Total Liabilities"),
        "net assets": (
            "TOTAL NET ASSETS",
            "Total net assets",
            "TOTAL NET ASSET VALUE",
            "Net assets at the end of the",
            "Net Assets at the end of the",
        ),
    },
    "it": {
        "assets": ("TOTALE ATTIVITA", "TOTALE ATTIVITÀ"),
        "liabilities": ("TOTALE PASSIVITA", "TOTALE PASSIVITÀ"),
        "net assets": (
            "VALORE COMPLESSIVO NETTO DEL FONDO",
            "Valore complessivo netto del fondo",
            "PATRIMONIO NETTO ATTRIBUIBILE",
        ),
    },
}


def rows_of(lines):
    rows = {}
    for line in lines:
        rows.setdefault(round(line.bbox[1] / ROW_TOLERANCE), []).append(line)
    return rows


def numbers_right_of(row, x):
    cells = sorted(
        (ln for ln in row if ln.bbox[0] > x and NUMBER.match(ln.text.strip())),
        key=lambda ln: ln.bbox[0],
    )
    return [cell.text.strip() for cell in cells]


def first_total(document, labels):
    for index in range(len(document)):
        lines = pdflines_from_pagedict(document[index].get_text("dict"))
        for _, row in sorted(rows_of(lines).items()):
            for line in row:
                if line.text.strip().startswith(labels):
                    return (
                        index + 1,
                        line.text.strip(),
                        numbers_right_of(row, line.bbox[2]),
                    )
    return None, None, []


def as_number(text, profile):
    digits = re.sub(r"[()\s %-]", "", text)
    if profile == "it":
        digits = digits.replace(".", "").replace(",", ".")
    else:
        digits = digits.replace(",", "")
    return float(digits)


def page_head(document, page, count=3):
    lines = [
        ln
        for ln in pdflines_from_pagedict(document[page - 1].get_text("dict"))
        if ln.text.strip()
    ]
    lines.sort(key=lambda ln: (ln.bbox[1], ln.bbox[0]))
    return [ln.text.strip() for ln in lines[:count]]


def main(argv):
    if not argv or argv[0] not in PROFILES:
        print(__doc__)
        return 2
    profile, paths = argv[0], argv[1:]
    if not paths:
        print(__doc__)
        return 2
    for path in paths:
        with pymupdf.open(path) as document:
            print(f"=== {path}  ({len(document)} pages)")
            found = {}
            for what, labels in PROFILES[profile].items():
                page, text, numbers = first_total(document, labels)
                found[what] = numbers
                print(
                    f"  {what:11s} p{page if page else '-'}: {text!r} -> {numbers if numbers else '-'}"
                )
            try:
                assets, liabilities, net = (
                    as_number(found[k][0], profile)
                    for k in ("assets", "liabilities", "net assets")
                )
                holds = abs(assets - abs(liabilities) - net) <= max(
                    1.0, abs(net) * 1e-6
                )
                print(
                    f"  equation   {'holds' if holds else 'does NOT hold'} on the first column"
                )
            except (IndexError, ValueError):
                print("  equation   -  (not all three totals have a number)")
            page, _, _ = first_total(document, PROFILES[profile]["assets"])
            if page:
                print(f"  page head  p{page}: {page_head(document, page)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
