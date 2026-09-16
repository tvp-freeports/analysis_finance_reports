"""Which SFDR article does each periodic disclosure declare, by its title and by its tick mark?

The European template has one version for Article 8 products and one for Article 9, and the title of
the page names it (``…financial products referred to in Article 8…``). The first question of the
template, *Did this financial product have a sustainable investment objective?*, says it again with a
tick: **Yes** means Article 9, **No** means Article 8. This probe reads both, for every page that
carries the question, and says whether they agree.

The tick is recognised in six shapes, because one document can mix them -- each disclosure is laid
out by whoever manages the sub-fund:

- an ``X`` on its own line, or at the start of the label line (``X Oui``, ``X No``);
- a character of a symbol font (a Wingdings checked box, ``3`` in Wingdings 2, ``U``/``C`` in a
  custom dingbat font) -- empty-box and bullet glyphs are ignored;
- a glyph from an OCR text layer (``✓`` in ``HiddenHorzOCR``);
- an image: when both boxes are images, the checked one is the one that differs, and is the heavier;
- a vector drawing inside the box, read with ``page.get_drawings()``.

Whatever its shape, the tick is assigned by **position**: the left half of the answer row is Yes,
the right half is No. The labels are found in English, Italian, French, Spanish, German and Danish,
and they do not need to match the language of the question.

Usage:
    freeports-dev probe run sfdr_title [--summary] PDF-OR-DIR...
    python probe_sfdr_title.py [--summary] PDF...

``--summary`` prints only the count per document, and the pages where the two readings disagree or
one of them is missing.

Known limits: images are read through the engine's page dictionary, which does not return every image
a page draws -- a checked box can be invisible to it (and to the engine). A title set in a font with
no Unicode map comes out as shifted letters and reads as absent. The shapes above were learnt on some
three hundred disclosures; a new layout may need a new one.
"""

import re
import sys

import pymupdf
from freeports.utils.pdf_extract import pdfimages_from_pagedict, pdflines_from_pagedict

QUESTION = re.compile(
    r"(have a sustainable\s+investment objective|aveva un obiettivo di investimento sostenibile"
    r"|ten[ií]a un objetivo de inversi[oó]n sostenible|avait-il un objectif d.investissement durable)",
    re.IGNORECASE,
)
QUESTION_TAIL = re.compile(
    r"(investment objective\?|investimento sostenibile\?)", re.IGNORECASE
)
TITLE = re.compile(r"(?:article|articolo|art[ií]culo|artikel)\s*(8|9)\b", re.IGNORECASE)
YES = {"yes", "sì", "si", "oui", "sí", "ja"}
NO = {"no", "non", "nej", "nein"}
MARK_PREFIX = re.compile(r"^\s*([Xx✓✔✗✘☒■])(\s|$)")
#: Empty boxes and bullets: glyphs that sit next to an answer without marking it.
NOT_MARKS = {
    chr(c)
    for c in (0x25A1, 0x2610, 0x2022, 0x26AB, 0xF06C, 0xF0A7, 0xF0B7, 0xF0A8, 0xF06F)
}
SYMBOL_FONT = re.compile(r"(wingding|dingbat|symbol|webding)", re.IGNORECASE)
BAND = 45.0
MAX_BOX = 14.0


def label_word(text):
    word = MARK_PREFIX.sub("", text).strip().lower()
    return word if word in YES | NO else None


def glyph_mark(text, font):
    stripped = text.strip()
    if MARK_PREFIX.match(text):
        return stripped[0]
    if (
        len(stripped) == 1
        and stripped not in NOT_MARKS
        and (ord(stripped) > 0x2000 or SYMBOL_FONT.search(font))
    ):
        return stripped
    return None


def read_page(page):
    page_dict = page.get_text("dict")
    lines = pdflines_from_pagedict(page_dict)
    questions = [
        ln
        for ln in lines
        if QUESTION.search(ln.text) and ln.bbox[1] < page.rect.height * 0.6
    ]
    questions = questions or [ln for ln in lines if QUESTION_TAIL.search(ln.text)]
    if not questions:
        return None
    question = min(questions, key=lambda ln: ln.bbox[1])
    top = question.bbox[3]

    title, title_text = None, ""
    for line in sorted(lines, key=lambda ln: ln.bbox[1]):
        if line.bbox[1] < question.bbox[1]:
            match = TITLE.search(line.text)
            if match:
                title, title_text = int(match.group(1)), line.text.strip()
                break
    reading = {
        "title": title,
        "title_text": title_text,
        "labels": "-",
        "mark": "-",
        "tick": None,
    }

    labels = [
        (ln, label_word(ln.text))
        for ln in lines
        if top - 2 <= ln.bbox[1] <= top + BAND and label_word(ln.text)
    ]
    yes = next((ln for ln, word in labels if word in YES), None)
    no = next((ln for ln, word in labels if word in NO), None)
    if not (yes and no):
        return reading
    reading["labels"] = f"{yes.text.strip()}/{no.text.strip()}"
    row = (yes.bbox[1] + yes.bbox[3]) / 2
    middle = (yes.bbox[0] + no.bbox[0]) / 2
    left, right = yes.bbox[0] - 60, no.bbox[2] + 60

    def near(y, x):
        return top - 2 <= y <= row + BAND and left <= x <= right

    texts = []
    for line in lines:
        glyph = glyph_mark(line.text, line.font_name)
        y = (line.bbox[1] + line.bbox[3]) / 2
        if glyph and near(y, line.bbox[0]):
            texts.append(
                (
                    abs(y - row),
                    line.bbox[0],
                    f"'{glyph}' U+{ord(glyph):04X} {line.font_name}",
                )
            )

    images = []
    for image in pdfimages_from_pagedict(page_dict):
        x0, y0, x1, y1 = image.bbox
        y = (y0 + y1) / 2
        if x1 - x0 <= MAX_BOX and y1 - y0 <= MAX_BOX and near(y, x0):
            data = bytes(image.data)
            images.append(
                (
                    abs(y - row),
                    x0,
                    f"image {x1 - x0:.0f}x{y1 - y0:.0f}pt",
                    hash(data),
                    y,
                    len(data),
                )
            )
    # The same image on both sides at one height is a pair of empty boxes; when the two differ, the
    # checked box is the heavier one.
    pairs = {
        a[3]
        for a in images
        for b in images
        if a[3] == b[3] and abs(a[4] - b[4]) < 3 and (a[1] < middle) != (b[1] < middle)
    }
    images = sorted(
        (i for i in images if i[3] not in pairs), key=lambda i: (-i[5], i[0])
    )
    images = [i[:3] for i in images[:1]]

    drawings = []
    for drawing in page.get_drawings():
        rect = drawing["rect"]
        dark = sum(drawing.get("fill") or (1, 1, 1)) < 0.6
        y = (rect.y0 + rect.y1) / 2
        if (
            dark
            and rect.width <= 16
            and rect.height <= 16
            and len(drawing["items"]) >= 3
            and near(y, rect.x0)
        ):
            drawings.append(
                (abs(y - row), rect.x0, f"drawing {rect.width:.0f}x{rect.height:.0f}pt")
            )

    candidates = texts or images or drawings
    if candidates:
        _, x, shape = min(candidates)
        reading["mark"] = f"{shape} at x={x:.0f} (middle {middle:.0f})"
        reading["tick"] = 9 if x < middle else 8
    return reading


def verdict(reading):
    if reading["tick"] is None and reading["title"] is None:
        return "neither read"
    if reading["tick"] is None:
        return "no tick read"
    if reading["title"] is None:
        return "no title read"
    return "agree" if reading["title"] == reading["tick"] else "DISAGREE"


def main(argv):
    summary = argv[:1] == ["--summary"]
    paths = argv[1:] if summary else argv
    if not paths:
        print(__doc__)
        return 2
    for path in paths:
        with pymupdf.open(path) as document:
            readings = []
            for index in range(len(document)):
                if QUESTION.search(document[index].get_text()):
                    reading = read_page(document[index])
                    if reading:
                        readings.append((index + 1, reading))
            counts = {}
            for _, reading in readings:
                counts[verdict(reading)] = counts.get(verdict(reading), 0) + 1
            tally = (
                ", ".join(f"{key} {value}" for key, value in sorted(counts.items()))
                or "none"
            )
            print(f"=== {path}  disclosures: {len(readings)}  ({tally})")
            for page, reading in readings:
                if summary and verdict(reading) == "agree":
                    continue
                title = f"Art. {reading['title']}" if reading["title"] else "-"
                tick = f"Art. {reading['tick']}" if reading["tick"] else "-"
                print(
                    f"  p{page}: title {title:7s} tick {tick:7s} {verdict(reading):12s} "
                    f"labels {reading['labels']}  mark {reading['mark']}"
                )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
