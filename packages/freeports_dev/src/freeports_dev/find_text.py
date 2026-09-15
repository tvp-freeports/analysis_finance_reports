"""Finding the page, which is the question every other development command presupposes.

``inspect-page`` starts from a page number and ``make-tests`` pins one. Nothing said where to get
it: a merger note lives on one page of eleven hundred, and finding it meant leaving the tooling and
opening the document by hand. That is the gap this module closes, and two decisions shape it.

**A search may be a regular expression; a selection may not.** :class:`PdfLineSelection` matches
substrings with ``^`` and ``$`` anchors deliberately — its algebra has to decide whether one
selection is a subset of, overlaps or is disjoint from another, and for regular expressions that
question is undecidable. A search answers yes or no about one line and combines with nothing, so it
is free to be a regular expression, and it should be: what one gropes for is precisely what one
cannot yet name exactly. The two must not be confused, which is why ``--text`` and ``--regex`` are
two options rather than one that guesses from the shape of the argument.

**A hit is yielded as it is found.** Six hundred documents is minutes of reading, and the first hit
is usually the one wanted. Searching is therefore a generator and the caller prints as it goes,
which is also why there is no progress meter here: output that arrives steadily is the progress
meter.

The pages are read through ``pdflines_from_pagedict``, the same function the engine reads them
with. A development tool that sees a page differently from the engine sends its user looking for a
difference that is in the tool.
"""

import re
from dataclasses import dataclass
from pathlib import Path


class SearchError(Exception):
    """A search that cannot be run: a bad pattern, a bad range, nothing to search."""


@dataclass(frozen=True)
class Hit:
    """One matching line, and where it was found."""

    document: Path
    page: int
    line: object


def text_predicate(text=None, regex=None, ignore_case=True):
    """A predicate over a line's text.

    Exactly one of `text` and `regex` is given. Neither is an error rather than a search matching
    every line, and both is an error rather than a precedence nobody would remember: a search that
    quietly ignored half of what it was asked is a search whose result means nothing.
    """
    if text is not None and regex is not None:
        raise SearchError(
            "--text and --regex were both given; a search takes one or the other"
        )
    if text is None and regex is None:
        raise SearchError("nothing to look for: give --text or --regex")
    if regex is not None:
        try:
            compiled = re.compile(regex, re.IGNORECASE if ignore_case else 0)
        except re.error as error:
            raise SearchError(
                f"invalid regular expression '{regex}': {error}"
            ) from error
        return lambda value: compiled.search(value) is not None
    if ignore_case:
        needle = text.lower()
        return lambda value: needle in value.lower()
    return lambda value: text in value


def font_predicate(font):
    """A predicate over a line's font: a substring of the font name, or everything.

    Case-insensitive, because the engine normalises font names to lower case in some paths and
    leaves them as the PDF wrote them in others, and a search is not the place to care which.
    """
    if not font:
        return lambda line: True
    needle = font.lower()
    return lambda line: needle in line.font_name.lower()


def parse_pages(spec):
    """``"7"``, ``"10-20"``, ``"900-"``, ``"-40"`` or ``None`` as a one-based inclusive range.

    The upper end may be absent, which means the end of the document — the shape wanted when
    looking for something one knows is in the back matter.
    """
    if spec is None or spec == "":
        return (1, None)
    text = str(spec).strip()
    if "-" not in text:
        if not text.isdigit():
            raise SearchError(
                f"invalid page range '{spec}': expected a number or 'first-last'"
            )
        return (int(text), int(text))
    first, _, last = text.partition("-")
    first, last = first.strip(), last.strip()
    if (first and not first.isdigit()) or (last and not last.isdigit()):
        raise SearchError(
            f"invalid page range '{spec}': expected a number or 'first-last'"
        )
    start = int(first) if first else 1
    end = int(last) if last else None
    if end is not None and end < start:
        raise SearchError(
            f"invalid page range '{spec}': {start}-{end} ends before it starts"
        )
    return (start, end)


def page_numbers(page_range, total):
    """The one-based page numbers of `page_range` that a document of `total` pages actually has.

    One-based because every other command of this tool is, and a search whose answer had to be
    adjusted before being passed to ``inspect-page`` would be a search that causes the mistake it
    exists to prevent.
    """
    start, end = page_range
    last = total if end is None else min(end, total)
    return iter(range(start, last + 1)) if start <= last else iter(())


def _pdfs_under(directory):
    return sorted(path for path in directory.rglob("*.pdf") if path.is_file())


def _documents_of_format(repo, format_name):
    """The reports a format's own test corpus holds: one, or one per numbered variant."""
    base = Path(repo) / "tests" / "formats" / format_name
    single = base / "report.pdf"
    if single.is_file():
        return [single]
    found = sorted(path for path in base.glob("*/report.pdf") if path.is_file())
    if not found:
        raise SearchError(
            f"no report found for format '{format_name}': looked for {single} and {base}/*/report.pdf"
        )
    return found


def documents_to_search(paths, repo=None, format_name=None):
    """The documents a search will read, in the order they were asked for.

    A path names a PDF or a directory to walk; a format name resolves against the repository's own
    test corpus, which is where a format author's documents already are. A directory holding no PDF
    is an error, not an empty search: a search that silently reads nothing reports "not found" for
    a document it never opened, which is the one answer a search must never give wrongly.
    """
    found = []
    for candidate in paths or ():
        path = Path(candidate)
        if path.is_dir():
            under = _pdfs_under(path)
            if not under:
                raise SearchError(f"no PDF under {path}")
            found.extend(under)
        elif path.is_file():
            found.append(path)
        else:
            raise SearchError(f"no such file or directory: {path}")

    if not found and format_name:
        found = _documents_of_format(repo, format_name)

    if not found:
        raise SearchError(
            "nothing to search: name a PDF, a directory, or a format with --format"
        )

    unique = []
    for path in found:
        if path not in unique:
            unique.append(path)
    return unique


def _page_lines(document, number):
    """The lines of one page, read exactly as the engine reads them."""
    from freeports.utils.pdf_extract import pdflines_from_pagedict

    return pdflines_from_pagedict(document[number - 1].get_text("dict"))


def search(
    documents, match_text, match_font=None, pages=None, max_hits=None, loader=None
):
    """Yield a :class:`Hit` for every matching line, document by document and page by page.

    `loader` opens a document and is a parameter so that the search can be exercised without a PDF;
    the default is PyMuPDF, which this package already depends on.
    """
    import pymupdf

    match_font = match_font or (lambda line: True)
    page_range = pages or (1, None)
    loader = loader or (lambda path: pymupdf.Document(str(path)))

    emitted = 0
    for path in documents:
        document = loader(path)
        for number in page_numbers(page_range, len(document)):
            for line in _page_lines(document, number):
                if not match_text(line.text) or not match_font(line):
                    continue
                yield Hit(Path(path), number, line)
                emitted += 1
                if max_hits and emitted >= max_hits:
                    return


def display_path(path):
    """`path` relative to the working directory when it is under it, and whole otherwise.

    A search prints one row per hit and the path is the widest column in it; an absolute path
    repeated fifty times pushes the text — the reason one is reading — off the right of the
    terminal. Relative only when the file really is below the working directory, because a path
    that cannot be pasted into the next command is worse than a long one.
    """
    try:
        return str(Path(path).resolve().relative_to(Path.cwd()))
    except ValueError:
        return str(path)


def format_hit(hit, show_font=True, text_width=None):
    """One hit as one line of terminal output, starting with what to type into ``inspect-page``."""
    x0, y0, x1, _ = hit.line.bbox
    text = hit.line.text
    if text_width and len(text) > text_width:
        text = text[:text_width] + "…"
    where = f"{display_path(hit.document)}:{hit.page}"
    geometry = f"y={y0:7.1f} x={x0:6.1f}-{x1:6.1f}"
    if not show_font:
        return f"{where}  {geometry}  {text}"
    return f"{where}  {geometry}  {hit.line.font_name}[{hit.line.font_size:g}]  {text}"
