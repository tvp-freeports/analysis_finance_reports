"""Showing a page on a terminal: the lines as a table, the images as pictures.

A page is not drawable on a terminal and an image is. That asymmetry is the whole design of this
module, and the two halves below follow from it.

Why the page is not drawn
=========================

Mapping the page box onto a character grid fails in three separate ways at once. Two lines can
overlap and a cell holds one character. A 6pt caption and a 24pt heading occupy the same cell
height, so the one visual cue a reader would actually use is the one the grid destroys. And the
metadata that makes a line *selectable* -- its font, its corpus, its box -- does not fit beside the
text it describes, so a drawing either omits it or is mostly metadata. A picture wrong in all three
ways is worse than none, because it is read as though it were right.

So the geometry is **reported**, not imitated: one row per line, with the numbers. Three things
make that readable rather than merely honest.

**The font legend.** A page carries three to eight distinct (font, corpus) pairs across several
hundred lines. Naming each pair once in a header and tagging its lines with a letter costs a few
rows and returns the whole terminal width to the text. The header is not a tax either: it is
precisely what one needs in order to write a :class:`PdfLineSelection`, which is why one is reading
the page at all.

**Overlap is signalled, not drawn.** A line whose box meets another's is marked. Where the two sit
relative to each other is already in their coordinates; that they collide is the part a reader
would otherwise have to discover by arithmetic.

**Reading order is the caller's choice.** Many pages of these reports carry two tables side by
side, and ordering by ordinate alone interleaves them into nonsense: row one of the left table,
row one of the right, row two of the left. Top-to-bottom is a default, not a truth, so the
column-major and banded orders exist for the pages it lies about.

Why the images are drawn
========================

An image has no competing text to collide with and no corpus to misrepresent, and at twenty
columns a tick mark is already recognisable. The one thing that must be got right is *which*
pixels: the bytes embedded in the page dictionary are frequently not what a reader sees --
in these reports a tick mark is stored as a solid black rectangle whose shape lives in a separate
mask the dictionary does not carry. What is previewed is therefore the **rendered page region**,
mask and transparency already applied. The functions here take grey levels and know nothing about
where they came from, which is what keeps that decision in one place, at the caller.
"""

import math
from dataclasses import dataclass

#: Grey levels map onto these, lightest first. Ten steps is as much as a terminal font distinguishes.
INK_RAMP = " .:-=+*#%@"

#: How far apart two lines may start, vertically, and still be one row of one table. Corpora in
#: these documents arrive with a long decimal tail, and 231.9 and 232.0 are one row.
ROW_TOLERANCE = 2.0

#: The same, for grouping lines into columns when reading column-major.
COLUMN_TOLERANCE = 2.0

#: A character cell is about twice as tall as it is wide, so a preview halves its rows to keep an
#: image from coming out stretched.
CELL_ASPECT = 2.0


@dataclass(frozen=True)
class FontEntry:
    """One row of the legend: a tag, and the (font, corpus) pair it stands for."""

    tag: str
    font_name: str
    font_size: float


@dataclass(frozen=True)
class NearestText:
    """The line closest to a box, which direction it lies in, and how far away it is."""

    line: object
    direction: str
    distance: float


def _tag_for(index):
    """``A``…``Z``, then ``AA``, ``AB``… — spreadsheet columns, for the same reason."""
    out = ""
    index += 1
    while index:
        index, remainder = divmod(index - 1, 26)
        out = chr(ord("A") + remainder) + out
    return out


def _size_key(size):
    """Corpora are compared to the hundredth: 8.980199813842773 and 8.980201 are one corpus."""
    return round(float(size), 2)


def font_legend(lines):
    """The distinct (font, corpus) pairs of `lines`, tagged in order of first appearance.

    First appearance rather than alphabetical order, so that re-reading a page after an edit does
    not renumber every tag because one line changed font.
    """
    entries = []
    seen = {}
    for item in lines:
        key = (item.font_name, _size_key(item.font_size))
        if key in seen:
            continue
        seen[key] = True
        entries.append(FontEntry(_tag_for(len(entries)), key[0], key[1]))
    return entries


def legend_tags(entries):
    """The legend keyed by (font, rounded corpus), for looking a line's tag up."""
    return {(entry.font_name, entry.font_size): entry.tag for entry in entries}


def _cluster(values, tolerance):
    """Assign each value the index of its cluster, clustering greedily along the sorted order.

    Greedy rather than a fixed grid: a grid puts 231.9 and 232.0 in different buckets whenever the
    boundary happens to fall between them, which is exactly the case this exists to get right.
    """
    order = sorted(range(len(values)), key=lambda i: values[i])
    clusters = [0] * len(values)
    index = 0
    start = None
    for position in order:
        value = values[position]
        if start is None:
            start = value
        elif value - start > tolerance:
            index += 1
            start = value
        clusters[position] = index
    return clusters


def _page_width_of(lines, page_width):
    if page_width:
        return float(page_width)
    return max((item.bbox[2] for item in lines), default=1.0) or 1.0


def _band_of(item, page_width, columns):
    band = int(item.bbox[0] / (page_width / columns))
    return max(0, min(columns - 1, band))


def reading_order(lines, order="y", columns=1, page_width=None):
    """`lines` in the order asked for.

    `order` is ``"y"`` -- top to bottom, then left to right -- or ``"x"``, which reads each column
    of the page down before moving right. `columns` splits the page into that many vertical bands
    and finishes each band before starting the next, which is what a page carrying two tables side
    by side needs: ordering such a page by ordinate alone interleaves the two tables' rows.

    An order this function does not know is an error, not a fallback to the default. A sort order
    silently replaced by another is a page read in an order nobody chose.
    """
    if order not in ("y", "x"):
        raise ValueError(f"unknown reading order {order!r}: expected 'y' or 'x'")
    if not lines:
        return []

    lines = list(lines)
    rows = _cluster([item.bbox[1] for item in lines], ROW_TOLERANCE)
    cols = _cluster([item.bbox[0] for item in lines], COLUMN_TOLERANCE)

    if columns and columns > 1:
        width = _page_width_of(lines, page_width)
        bands = [_band_of(item, width, columns) for item in lines]
    else:
        bands = [0] * len(lines)

    def key(index):
        item = lines[index]
        if order == "y":
            return (bands[index], rows[index], item.bbox[0])
        return (bands[index], cols[index], item.bbox[1])

    return [lines[i] for i in sorted(range(len(lines)), key=key)]


def overlapping_indices(lines):
    """The indices of the lines whose box meets another line's box.

    Boxes that merely touch along an edge do not count: adjacent cells of a table share a boundary
    by construction, and marking every one of them would mark the whole page.
    """
    lines = list(lines)
    order = sorted(range(len(lines)), key=lambda i: lines[i].bbox[1])
    found = set()
    for position, index in enumerate(order):
        ax0, ay0, ax1, ay1 = lines[index].bbox
        for other in order[position + 1 :]:
            bx0, by0, bx1, by1 = lines[other].bbox
            if by0 >= ay1:
                break
            if ax0 < bx1 and bx0 < ax1 and ay0 < by1 and by0 < ay1:
                found.add(index)
                found.add(other)
    return found


def show_codepoints(text):
    """`text` with every non-printable-ASCII character replaced by ``<U+XXXX>``.

    This is what tells a box glyph from a letter. A checkbox in these reports is sometimes
    ``U+2612``, sometimes ``U+26AB`` in a symbol font, and on a terminal both are a smudge or a
    replacement square.
    """
    return "".join(ch if 32 <= ord(ch) <= 126 else f"<U+{ord(ch):04X}>" for ch in text)


def render_lines(
    lines, order="y", columns=1, page_width=None, text_width=None, codepoints=False
):
    """The page as rows of text: a font legend, a header, then one row per line.

    Returns the rows rather than printing them, so the caller decides where they go and the
    arrangement can be tested without capturing output.
    """
    lines = list(lines)
    if not lines:
        return ["no lines on this page"]

    entries = font_legend(lines)
    tags = legend_tags(entries)
    ordered = reading_order(lines, order=order, columns=columns, page_width=page_width)
    overlaps = overlapping_indices(ordered)

    out = [f"fonts ({len(entries)} distinct):"]
    out.extend(
        f"  {entry.tag:<3} {entry.font_name:<28} {entry.font_size:g}"
        for entry in entries
    )

    described = (
        "top to bottom, then left to right"
        if order == "y"
        else "down each column, then right"
    )
    banded = f", in {columns} vertical bands" if columns and columns > 1 else ""
    out.append(f"order: {order} ({described}{banded})")
    if overlaps:
        out.append("  ! = this line's box overlaps another line's box")
    out.append(f"{'y':>8}{'x0':>8}{'x1':>8}  f   text")

    for index, item in enumerate(ordered):
        x0, y0, x1, _ = item.bbox
        text = show_codepoints(item.text) if codepoints else item.text
        if text_width and len(text) > text_width:
            text = text[:text_width] + "…"
        tag = tags[(item.font_name, _size_key(item.font_size))]
        row = f"{y0:8.1f}{x0:8.1f}{x1:8.1f}  {tag:<3} {text}"
        out.append(row + " !" if index in overlaps else row)
    return out


def resample(gray, width, height, cols, rows):
    """`gray` reduced to `cols` × `rows` by averaging each source block.

    Averaging rather than sampling: a tick mark one pixel wide disappears entirely from a preview
    that picks one source pixel per output cell, and disappearing is the one failure a preview must
    not have.
    """
    out = []
    for row in range(rows):
        y0 = row * height // rows
        y1 = max(y0 + 1, (row + 1) * height // rows)
        line = []
        for col in range(cols):
            x0 = col * width // cols
            x1 = max(x0 + 1, (col + 1) * width // cols)
            block = [gray[y * width + x] for y in range(y0, y1) for x in range(x0, x1)]
            line.append(int(round(sum(block) / len(block))))
        out.append(line)
    return out


def ascii_preview(gray, width, height, cols=20):
    """`gray` — row-major grey levels, 0 black to 255 white — as rows of characters.

    The row count is derived from the aspect ratio and then halved, because a character cell is
    about twice as tall as it is wide and an un-halved preview comes out stretched.
    """
    rows = max(1, int(round(height / width * cols / CELL_ASPECT))) if width else 1
    return [
        "".join(
            INK_RAMP[min(len(INK_RAMP) - 1, (255 - value) * len(INK_RAMP) // 256)]
            for value in row
        )
        for row in resample(gray, width, height, cols, rows)
    ]


def ink_ratio(gray):
    """How dark `gray` is on average, 0 for white and 1 for black.

    A cheap answer to "is this box ticked", which is the question most often asked of a small image
    in these documents.
    """
    if not gray:
        return 0.0
    return sum(255 - value for value in gray) / (255.0 * len(gray))


def _gap(a0, a1, b0, b1):
    """How far two intervals are apart, zero when they overlap."""
    if b0 > a1:
        return b0 - a1
    if a0 > b1:
        return a0 - b1
    return 0.0


def nearest_lines(bbox, lines, count=3):
    """The `count` lines closest to `bbox`, nearest first.

    What an image *means* is whatever is written beside it: the tick mark of an SFDR form is
    identified by the ``Yes`` or ``No`` it sits next to, never by its own appearance.

    More than one, because the nearest line is often not the informative one. A tick in a form sits
    a few points below its section heading and a dozen points left of its own label, so the single
    closest line is the heading — true, and not what was being asked. Three lines cost three rows
    and take the guessing out of it.

    The direction is decided by which gap is the larger, so a label on the same row as the image
    reads as ``left`` or ``right`` even when it is also very slightly higher.
    """
    x0, y0, x1, y1 = bbox
    found = []
    for item in lines:
        bx0, by0, bx1, by1 = item.bbox
        dx = _gap(x0, x1, bx0, bx1)
        dy = _gap(y0, y1, by0, by1)
        if dx >= dy:
            direction = "right" if bx0 > x1 else "left"
        else:
            direction = "below" if by0 > y1 else "above"
        found.append(NearestText(item, direction, math.hypot(dx, dy)))
    found.sort(key=lambda near: near.distance)
    return found[:count]


def nearest_line(bbox, lines):
    """The single line closest to `bbox`, or ``None`` when the page has no text."""
    found = nearest_lines(bbox, lines, count=1)
    return found[0] if found else None


# -- the half that touches a PDF -------------------------------------------------------------
# Everything above takes numbers and returns strings, which is what makes it testable without a
# document. What follows opens one. The import is local for the same reason it is local in
# `find_text.search`: importing PyMuPDF costs a measurable fraction of a second, and most
# subcommands never need it.


def page_images(page_dict):
    """The raster images of a page, read exactly as the engine reads them.

    Through the engine's own extractor rather than by walking the dictionary here, so that an image
    this tool shows is an image a pipe can see — including the degenerate boxes the engine drops.
    """
    from freeports.utils.pdf_extract import pdfimages_from_pagedict

    return pdfimages_from_pagedict(page_dict)


def rendered_gray(pdf_page, bbox, dpi=150):
    """The page region under `bbox`, rendered to grey levels: what a reader actually sees there.

    **Not** the bytes the page dictionary carries for the image. Those are frequently not the
    picture: in these reports a tick mark is stored as a solid black rectangle whose shape lives in
    a separate mask the dictionary does not carry, so previewing the stored bytes shows a black
    square for every tick on the page. Rendering the region applies the mask, the transparency and
    anything drawn over or under it, which is the only definition of "what is there" that matches
    the person checking the PDF by eye.
    """
    import pymupdf

    pixmap = pdf_page.get_pixmap(
        clip=pymupdf.Rect(*bbox), dpi=dpi, colorspace=pymupdf.csGRAY
    )
    return list(pixmap.samples), pixmap.width, pixmap.height


def stored_size(image):
    """The stored image's size in pixels, as ``"13x13 px"``, or ``"? px"`` if it will not decode.

    Worth a column of its own beside the box: a 13-pixel image drawn into a 9-point box is a glyph
    scaled up, and one drawn into a 200-point box is a scan. That ratio is the first thing that
    tells you which kind of thing you are looking at.

    A failure to decode is reported and not raised. Some stored streams are not standalone images
    at all, and the point of this view is to look at a page one does not yet understand.
    """
    import pymupdf

    try:
        pixmap = pymupdf.Pixmap(bytes(image.data))
    except Exception:  # noqa: BLE001 -- an undecodable stream is a fact to print, not a failure
        return "? px"
    return f"{pixmap.width}x{pixmap.height} px"


def document_image_counts(document):
    """How many times each image digest occurs in the whole document.

    A whole-document walk, so it is asked for rather than assumed: on a report of eleven hundred
    pages it is seconds, and the per-page count answers most questions.
    """
    import hashlib

    counts = {}
    for page in document:
        for image in page_images(page.get_text("dict")):
            digest = hashlib.sha256(bytes(image.data)).hexdigest()
            counts[digest] = counts.get(digest, 0) + 1
    return counts


def render_images(
    pdf_page,
    page_dict,
    lines,
    cols=20,
    dpi=150,
    document_counts=None,
    save_dir=None,
    page_number=None,
):
    """The page's raster images as rows of terminal output.

    Each image gets its box, its size on the page and in pixels, its digest with how often that
    same digest occurs, the nearest text with the direction it lies in, and a preview of the
    rendered region with its ink ratio.

    The digest is there because it answers the first question one has about a small repeated image:
    the five marks of an SFDR form on one page turn out to be one image used five times, and a
    single column of output says so. The nearest text is there because it is what the image
    *means*: a tick is identified by the ``Yes`` or ``No`` beside it, never by its own appearance.
    """
    import hashlib

    images = list(page_images(page_dict))
    if not images:
        return ["no raster images on this page"]

    digests = [hashlib.sha256(bytes(image.data)).hexdigest() for image in images]
    on_page = {digest: digests.count(digest) for digest in set(digests)}

    out = [f"{len(images)} raster image(s)"]
    for index, (image, digest) in enumerate(zip(images, digests)):
        x0, y0, x1, y1 = image.bbox
        out.append("")
        out.append(
            f"[{index}] bbox=({x0:.1f}, {y0:.1f}, {x1:.1f}, {y1:.1f})  {x1 - x0:.1f}x{y1 - y0:.1f} pt"
        )

        repeats = f"{on_page[digest]}x on this page"
        if document_counts is not None:
            repeats += f", {document_counts.get(digest, 0)}x in the document"
        out.append(
            f"     {image.ext}  {stored_size(image)}  {len(image.data)} B  sha256 {digest[:16]}  ({repeats})"
        )

        for position, near in enumerate(nearest_lines((x0, y0, x1, y1), lines)):
            label = "nearest text:" if position == 0 else " " * 13
            out.append(
                f'     {label} "{near.line.text}" ({near.direction}, {near.distance:.1f} pt)'
            )

        gray, width, height = rendered_gray(pdf_page, (x0, y0, x1, y1), dpi=dpi)
        if width and height:
            out.append(
                f"     rendered region {width}x{height} px, ink {ink_ratio(gray):.2f}"
            )
            out.extend(
                "       " + row for row in ascii_preview(gray, width, height, cols=cols)
            )

        if save_dir is not None:
            out.extend(
                _save_image(
                    save_dir, page_number, index, image, pdf_page, (x0, y0, x1, y1), dpi
                )
            )
    return out


def _save_image(save_dir, page_number, index, image, pdf_page, bbox, dpi):
    """Write both the stored bytes and the rendered region, and say where they went.

    Both, because they differ and the difference is the point: comparing the two is how one finds
    out that the stored image is a mask away from what the page shows.
    """
    import pymupdf

    from pathlib import Path

    directory = Path(save_dir)
    directory.mkdir(parents=True, exist_ok=True)
    stem = f"p{page_number}-{index}" if page_number is not None else f"{index}"

    stored = directory / f"{stem}.{image.ext}"
    stored.write_bytes(bytes(image.data))

    rendered = directory / f"{stem}.render.png"
    pdf_page.get_pixmap(clip=pymupdf.Rect(*bbox), dpi=dpi).save(str(rendered))
    return [f"     saved {stored} and {rendered}"]
