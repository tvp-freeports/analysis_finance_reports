"""Which tests a formats repository *has* — read once, by the suite and by the report alike.

pytest discovers the tests in a formats repository by walking directories and reading file names:
a page with a ``<n>-pdf_blks.json`` beside a ``report.pdf`` yields a pdf-extract test, a document
with an ``out/`` yields the whole-document test, and so on. The coverage figures
``tests.formats.integration`` and ``tests.formats.single_page`` are statements about exactly that
walk — what fraction of the documents in this repository are tested at all.

So the walk lives here and both callers read it. Two implementations of "which tests exist" would
be two implementations that rot apart, and the day they disagree is the day the report is lying
about the suite: it would say a document is covered while pytest collects nothing for it, and
nobody would find out, because the report is the only thing anybody looks at.

Nothing in this module imports pandas, PyMuPDF or the engine. It is a directory walk and a set of
file names, which is what lets ``freeports-dev coverage`` stay fast enough to run at every commit
while the suite it describes takes minutes.

**The unit is the document, not the format.** A format with one ``report.pdf`` is one document; a
format whose directory holds subdirectories instead is one document per subdirectory. That is the
same split :class:`freeports_dev.pytest_plugin.ReportVariant` is created on, and it is why the
figure counts 36 documents in a repository holding fewer formats than that.
"""

import os
from pathlib import Path


#: The three fixtures a page can carry, by the file-name suffix that declares each one.
PDF_BLKS = "pdf_blks.json"
TXT_BLKS = "txt_blks.json"
RESULTS = "results.json"

#: A page file that configures the test rather than being a fixture of it.
FILTER_DATA = "filter_data.json"

#: Subdirectories of a format directory that are part of a single document, not documents of their own.
_NOT_A_VARIANT = ("pages", "out")


class InventoryError(Exception):
    """A document the rules cannot describe: no ``report.pdf``, no ``pages/``, a stray file.

    Raised only when :func:`scan_variant` is asked to be strict, which is what the pytest plugin
    asks for: a repository pytest cannot collect must fail loudly at collection. The coverage
    report asks for the opposite and records the problem as a finding, so that a repository with
    one malformed document can still be measured — a report that refuses to print because one
    document is broken tells you less than one that prints and names it.
    """


class DocumentInventory:
    """One document, and every test the walk says exists for it.

    ``variant`` is ``None`` for a format holding a single ``report.pdf``, and the subdirectory's
    name for a format holding several. ``name`` is what a report prints, and it is the same string
    the status notes use — ``EURIZON-IT24/3``.
    """

    def __init__(self, format_name, variant, path):
        self.format_name = format_name
        self.variant = variant
        self.path = Path(path)
        self.has_report = False
        self.has_out = False
        self.has_pages_dir = False
        self.pages_by_type = {}
        self.pdf_blks = set()
        self.txt_blks = set()
        self.results = set()
        self.problems = []

    @property
    def name(self):
        return (
            f"{self.format_name}/{self.variant}" if self.variant else self.format_name
        )

    @property
    def all_pages(self):
        pages = set()
        for numbers in self.pages_by_type.values():
            pages |= numbers
        return pages

    # -- what pytest would collect ----------------------------------------------------------

    @property
    def pdf_extract_pages(self):
        """Pages yielding a pdf-extract test: the blocks fixture, and a document to extract from."""
        return self.pdf_blks & self.all_pages if self.has_report else set()

    @property
    def text_filter_pages(self):
        """Pages yielding a text-filter test: the stage's input fixture and its output fixture."""
        return self.pdf_blks & self.txt_blks & self.all_pages

    @property
    def deserialize_pages(self):
        """Pages yielding a deserialize test, by the same rule one stage further down."""
        return self.txt_blks & self.results & self.all_pages

    @property
    def has_integration(self):
        """Whether the whole-document test is collected at all.

        This is the whole of ``tests.formats.integration``: the metric asks whether a document is
        checked end to end, and a document with no ``out/`` has no expected output to be checked
        against, so pytest yields nothing for it.
        """
        return self.has_report and self.has_out

    @property
    def triple_pages(self):
        """Pages carrying all three fixtures, so that every stage of the pipeline is pinned there."""
        return self.pdf_blks & self.txt_blks & self.results

    @property
    def has_single_page_triple(self):
        """Whether at least one page carries the whole triple — ``tests.formats.single_page``.

        One such page is the bar, and deliberately so: it means every stage of the pipeline has a
        fixture somewhere in this document, which is what distinguishes a document somebody tested
        from a document somebody started testing.
        """
        return bool(self.triple_pages)


def variant_paths(format_dir):
    """The documents of one format directory, as ``(variant, path)`` pairs.

    A directory holding a ``report.pdf`` is one document and its variant is ``None``. A directory
    holding subdirectories other than ``pages`` and ``out`` is one document per subdirectory. This
    is the split :meth:`FreeportsFormat.collect` makes, and it is made here so that both it and the
    coverage walk count the same documents.
    """
    format_dir = Path(format_dir)
    if not format_dir.is_dir():
        return []
    subdirectories = []
    single = False
    for entry in sorted(os.listdir(format_dir)):
        if (format_dir / entry).is_dir():
            if entry not in _NOT_A_VARIANT:
                subdirectories.append(entry)
        elif entry == "report.pdf" and (format_dir / entry).is_file():
            single = True
    if single:
        return [(None, format_dir)]
    return [(variant, format_dir / variant) for variant in subdirectories]


def scan_variant(path, format_name, variant=None, strict=False):
    """Walk one document and record every fixture it holds.

    ``strict`` is what the pytest plugin passes: a repository pytest cannot collect must fail at
    collection rather than be quietly under-collected. The coverage report passes ``False`` and
    reads :attr:`DocumentInventory.problems` instead.
    """
    inventory = DocumentInventory(format_name, variant, path)
    directory = Path(path)

    inventory.has_report = (directory / "report.pdf").exists()
    inventory.has_out = (directory / "out").exists()

    if not inventory.has_report:
        message = f"Missing report.pdf in {directory}"
        if strict:
            raise InventoryError(message)
        inventory.problems.append(message)

    pages_dir = directory / "pages"
    inventory.has_pages_dir = pages_dir.exists()
    if not inventory.has_pages_dir:
        message = f"Missing pages directory in {directory}"
        if strict:
            raise InventoryError(message)
        inventory.problems.append(message)
        return inventory

    for page_type in os.listdir(pages_dir):
        type_dir = pages_dir / page_type
        if not type_dir.is_dir():
            continue

        inventory.pages_by_type[page_type] = set()

        for entry in os.listdir(type_dir):
            if "-" not in entry:
                continue
            page_number_text, _, file_type = entry.partition("-")
            try:
                page_number = int(page_number_text)
            except ValueError:
                continue

            inventory.pages_by_type[page_type].add(page_number)

            if file_type == PDF_BLKS:
                inventory.pdf_blks.add(page_number)
            elif file_type == TXT_BLKS:
                inventory.txt_blks.add(page_number)
            elif file_type == RESULTS:
                inventory.results.add(page_number)
            elif file_type in (FILTER_DATA):
                # `in` against a string, not a tuple, so this also passes any file whose name is a
                # substring of `filter_data.json`. Kept exactly as the plugin has always had it:
                # tightening it would make a repository that collects today start failing, which is
                # not a change to slip into a refactoring. Reported, not fixed here.
                pass
            else:
                message = f"Unknown file in pages folder: {entry}"
                if strict:
                    raise InventoryError(message)
                inventory.problems.append(message)

    numbered = [
        number for numbers in inventory.pages_by_type.values() for number in numbers
    ]
    if len(numbered) != len(set(numbered)):
        message = f"Found pages classified in multiple ways in {directory}"
        if strict:
            raise InventoryError(message)
        inventory.problems.append(message)

    return inventory


def scan_repository(repo, formats=None):
    """Every document of a formats repository, in the order a report should list them.

    ``formats`` is the set of format names the repository declares; when it is ``None`` the
    repository's own ``metadata/formats.csv`` is read, so that a directory under ``tests/formats/``
    which the repository does not declare is not counted — pytest would not collect it either.
    """
    repo = Path(repo)
    if formats is None:
        formats = read_declared_formats(repo)
    formats = set(formats)

    root = repo / "tests" / "formats"
    documents = []
    if not root.is_dir():
        return documents
    for format_name in sorted(os.listdir(root)):
        if format_name not in formats:
            continue
        for variant, path in variant_paths(root / format_name):
            documents.append(scan_variant(path, format_name, variant, strict=False))
    return documents


def read_declared_formats(repo):
    """The format names this repository declares, through the engine's own reader.

    A format's name is not a column of ``metadata/formats.csv``: it is composed from three of them,
    ``AMUNDI`` + ``EN`` + ``24`` making ``AMUNDI-EN24``. Composing it again here would be a second
    implementation of the engine's rule, which is precisely the duplication this module exists to
    remove one of — and a coverage report that names its documents differently from the suite is a
    report about a different repository.

    The engine import this costs was measured rather than assumed: 2 ms, against a walk that reads
    several hundred file names. It does not take this command out of a commit hook.
    """
    table = Path(repo) / "metadata" / "formats.csv"
    if not table.exists():
        return set()
    from freeports.formats_repo import get_formats

    return set(get_formats(Path(repo)))
