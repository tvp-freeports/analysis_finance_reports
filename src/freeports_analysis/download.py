"""Functions to download pdf from internet and for scraping"""

import logging as log
from io import BytesIO
import os
import requests as rq
from freeports_analysis.i18n import _

logger = log.getLogger(__name__)


def download_pdf(url: str, pdf: str = None) -> BytesIO:
    """Function to download pdf file from url

    Parameters
    ----------
    url : str
        unique resource identifier on internet
    pdf : str
        path of the file where to save the file in filesystem

    Returns
    -------
    BytesIO
        output byte stream with input output operation like file

    Raises
    ------
    Exception
        if the code returned from the http get call is an error code, an exception occurs
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
