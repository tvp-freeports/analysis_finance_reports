"""Who manages the investments: is a manager declared, and are there exceptions per sub-fund?

Reports rarely say "this company manages every sub-fund". What they print is one of four shapes,
and which one decides how the investment managers are read:

- **no investment manager at all**: every sub-fund is managed by the management company;
- **one investment manager, with no sub-funds named**: it manages every sub-fund, and the management
  company manages none;
- **a default and exceptions**: the first manager has no specification and the next ones say which
  sub-funds they manage (``For the following sub-funds:``, ``(Only in respect of the Sub-Fund …)``,
  ``… for <sub-fund>``);
- **delegation in prose**: ``The Management Company delegated to X the day to day portfolio
  management of Y.``

This probe prints the lines that look like a manager *label* -- at the start of a line, short, not a
table of contents entry, not the title of a manager's report -- with the lines under it in the same
column, and counts the pages that describe a delegation in prose.

Usage:
    freeports-dev probe run inv_managers PDF-OR-DIR...
    python probe_inv_managers.py PDF...

Known limits: a list of managers without any label (in a note, in body type) is found only if the
note's first line happens to look like one. At most 60 labels are printed per document. Sub-managers
(``SUB-INVESTMENT MANAGERS``) are printed like managers: whether they count is a decision about the
data, not something the page says.
"""

import re
import sys

import pymupdf
from freeports.utils.pdf_extract import pdflines_from_pagedict

LABEL = re.compile(
    r"^\s*((sub[- ]?)*investment\s+manag(er|ers|er\(s\)|ement)\b|investment\s+advis[eo]rs?\b"
    r"|delegated\s+investment|portfolio\s+managers?\b|cash\s+manager\s+and\s+portfolio\s+manager"
    r"|gestor[ei]?\s+delegat|gestione\s+delegata|delega\s+(di|della)\s+gestione"
    r"|gestore\s+degli\s+investimenti"
    r"|gestora\s+de\s+inversiones|asesor\s+de\s+inversiones|gestionnaire\s+financier)",
    re.IGNORECASE,
)
#: A capitalised label broken over two lines (``INVESTMENT`` / ``MANAGER DELEGATI``). Case-sensitive on
#: purpose: in lower case, ``investment`` alone on a line is the end of a sentence far more often.
SPLIT_LABEL = re.compile(r"^\s*(INVESTMENT|MANAGERS?\s+DELEGATI)\s*$")
NOT_A_LABEL = re.compile(
    r"(report|relazione|rapporto|informe|fee|commission|remuneration|\s\d+\s*$)",
    re.IGNORECASE,
)
PROSE = re.compile(
    r"(delegated\s+to\s+.{0,80}(portfolio|investment)\s+management"
    r"|has\s+(designated|appointed)\s+.{0,60}investment\s+managers?"
    r"|delegat[oaie]\s+(la\s+)?gestione|gestore\s+delegato|sub-delegat)",
    re.IGNORECASE | re.DOTALL,
)
MAX_LABELS = 60


def main(paths):
    if not paths:
        print(__doc__)
        return 2
    for path in paths:
        with pymupdf.open(path) as document:
            print(f"=== {path}  ({len(document)} pages)")
            shown = 0
            prose_pages = []
            for index in range(len(document)):
                text = document[index].get_text()
                if PROSE.search(text):
                    prose_pages.append(index + 1)
                if shown >= MAX_LABELS or not re.search(
                    r"(?i)(manag|gestor|gestion|advis|delega)", text
                ):
                    continue
                lines = pdflines_from_pagedict(document[index].get_text("dict"))
                sizes = sorted(ln.font_size for ln in lines) or [0]
                body = sizes[len(sizes) // 2]
                for line in lines:
                    label = line.text.strip()
                    if (
                        not (LABEL.search(label) or SPLIT_LABEL.search(label))
                        or NOT_A_LABEL.search(label)
                        or len(label) > 70
                        or line.font_size < body * 0.7
                    ):
                        continue
                    under = sorted(
                        (
                            ln
                            for ln in lines
                            if 0 < ln.bbox[1] - line.bbox[1] <= 75
                            and abs(ln.bbox[0] - line.bbox[0]) < 30
                            and ln.text.strip()
                        ),
                        key=lambda ln: ln.bbox[1],
                    )
                    values = " | ".join(ln.text.strip() for ln in under[:5])
                    print(
                        f"  p{index + 1} y={line.bbox[1]:.0f} x={line.bbox[0]:.0f} "
                        f"[{line.font_name} {line.font_size:.1f}] {label!r}"
                    )
                    print(f"      -> {values[:230] if values else '-'}")
                    shown += 1
                    if shown >= MAX_LABELS:
                        break
            if not shown:
                print("  -  no manager label")
            listed = ", ".join(map(str, prose_pages[:12])) + (
                " …" if len(prose_pages) > 12 else ""
            )
            print(
                f"  delegation in prose: {len(prose_pages)} page(s){': ' + listed if prose_pages else ''}"
            )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
