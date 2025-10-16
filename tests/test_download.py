from .conftest import out_dir, url_example_formats, root_dir
import pytest
from freeports_analysis import download as dw
from requests import ConnectionError


@pytest.mark.online_tests
def test_download_pdf_URL_NOT_FOUND():
    with pytest.raises(ConnectionError):
        dw.download_pdf("https://www.lkdjafdad.dkfljsa.org/documents/", "report.pdf")


@pytest.mark.online_tests
def test_download_pdf_200_OK_NO_SAVE():
    fmt = "EURIZON-EN23.A"
    pdf = dw.download_pdf(url_example_formats[fmt])
    pdf_reference = root_dir / "formats" / "algorithms" / fmt / "report.pdf"
    assert pdf.getvalue() == pdf_reference.read_bytes()


@pytest.mark.online_tests
def test_download_pdf_200_OK_SAVE():
    fmt = "ANIMA-EN23"
    pdf_saved = out_dir / f"report-{fmt}.pdf"
    pdf = dw.download_pdf(url_example_formats[fmt], pdf_saved)
    pdf_reference = root_dir / "formats" / "algorithms" / fmt / "report.pdf"
    assert pdf.getvalue() == pdf_reference.read_bytes()
    assert pdf_saved.read_bytes() == pdf_reference.read_bytes()
