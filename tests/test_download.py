import conftest
import pytest
from freeports_analysis import download as dw
from requests import ConnectionError


def test_download_pdf_URL_NOT_FOUND():
    with pytest.raises(ConnectionError):
        dw.download_pdf(
            "https://www.lkdjafdad.dkfljsa.org/documents/", "report.pdf", False
        )
