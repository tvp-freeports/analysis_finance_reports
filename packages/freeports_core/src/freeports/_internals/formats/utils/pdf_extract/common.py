"""Common routines used to build more complex algorithms"""

from freeports._internals.formats.utils.pdf_extract.pdf_blks_acquire import (
    pdflines_from_pagedict,
    PdfLineSelection,
)
from freeports._internals.core.classes import PdfBlock


class SelectExpectedText:
    """Selects expected text from PDF lines based on selection criteria."""

    selection: PdfLineSelection
    name: str

    def __init__(
        self, selection: PdfLineSelection, name: str = "expected text"
    ) -> None:
        """Initialize the text selector.

        Parameters
        ----------
        selection : PdfLineSelection
            The line selection criteria.
        name : str
            Descriptive name for error messages.
        """
        self.selection = selection
        self.name = name

    def __call__(self, lines: list) -> str:
        """Select and return the expected text from PDF lines.

        Parameters
        ----------
        lines : list
            List of PDF line objects.

        Returns
        -------
        str
            The text content of the selected line.

        Raises
        ------
        ExpectedPdfBlockNotFound
            If no matching line is found.
        """
        try:
            return self.selection.select(lines)[0].text
        except IndexError as exc:
            logger.error(exc)
            logger.debug("First lines where:")
            logger.debug(
                "%s",
                str(list(map(lambda x: x.text, lines))[: min(10, len(lines))]),
            )
            raise ExpectedPdfBlockNotFound(
                f'Pdf block during extraction of "{self.name}" not found'
            ) from exc


class ExtractTextPdfBlockOrFailPage:
    """Extracts a PDF block from a page or raises PageParseFail on failure."""

    extractor: SelectExpectedText
    type_block: Enum

    def __init__(
        self, selection: PdfLineSelection, name: str, type_block: Enum
    ) -> None:
        """Initialize the PDF block extractor.

        Parameters
        ----------
        selection : PdfLineSelection
            The line selection criteria.
        name : str
            Descriptive name for error messages.
        type_block : Enum
            The block type to assign to extracted blocks.
        """
        self.extractor = SelectExpectedText(selection, name)
        self.type_block = type_block

    def __call__(self, dict_root: dict) -> list[PdfBlock]:
        """Extract a PDF block from a page dict or raise PageParseFail.

        Parameters
        ----------
        dict_root : dict
            The PDF page dictionary.

        Returns
        -------
        list[PdfBlock]
            List containing the extracted PDF block.

        Raises
        ------
        PageParseFail
            If the expected text is not found on the page.
        """
        lines = pdflines_from_pagedict(dict_root)
        try:
            text = self.extractor(lines)
        except ExpectedPdfBlockNotFound as e:
            raise PageParseFail(e) from e
        return [PdfBlock(self.type_block, {}, text)]
