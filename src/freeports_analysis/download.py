"""Functions for downloading PDF files from the internet and web scraping."""

import logging as log
from io import BytesIO
import os
from pathlib import Path
from typing import Optional
import requests as rq
from freeports_analysis.i18n import _

logger = log.getLogger(__name__)


def download_pdf(url: str, pdf: Optional[Path] = None) -> BytesIO:
    """Download PDF file from URL and optionally save to local filesystem.

    Parameters
    ----------
    url : str
        Unique resource identifier on internet pointing to PDF file
    pdf : Optional[Path], optional
        Path where to save the PDF in filesystem, by default None

    Returns
    -------
    BytesIO
        Byte stream with input/output operations like a file object

    Raises
    ------
    requests.RequestException
        If the HTTP GET call fails or returns an error status code

    Notes
    -----
    If `pdf` is provided, the downloaded PDF will be saved to that path
    in addition to being returned as a BytesIO stream. The function uses
    a 10-second timeout for the HTTP request.
    """
    try:
        response = rq.get(url, timeout=10)
        response.raise_for_status()
    except Exception as e:
        logger.critical(e)
        raise e
    file = BytesIO(response.content)
    if pdf is not None:
        with pdf.open("wb") as f:
            f.write(file.read())
        logger.debug(_("File %s saved on disk [in %s]"), pdf, os.getcwd())
    return file
