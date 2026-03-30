import pytest
from pymupdf import Document
from pathlib import Path
import os
import dill
import pandas as pd
import yaml
import freeports_analysis as fra
import freeports_lib
from freeports_analysis.data import get_target_companies
from freeports_analysis.formats.data import VALID_FORMATS
from freeports_analysis.formats.algorithms import Algorithm
from tests.conftest import out_dir, targets, conf

df = get_target_companies(["TEST"])
test_companies = (
    freeports_lib.text_filter.matcher.CompanyMatchInfos.compile_from_pandas_df(df)
)


def get_current_dir(path):
    current_path = Path(path)
    current_dir = current_path.parent
    return current_dir


def get_pdf_document(path):
    current_dir = get_current_dir(path)
    pdf = Document(current_dir / "report.pdf")
    pages = [p.get_text("dict") for p in pdf]
    return pages


def get_format_name(path):
    current_dir = get_current_dir(path)
    fmt = os.path.split(current_dir)[-1]
    if fmt not in VALID_FORMATS:
        fmt = os.path.split(current_dir.parent)[-1]
    return fmt


def get_algorithm(path):
    format_name = get_format_name(path)
    return Algorithm.load(format_name)


def get_page_types(path):
    current_dir = get_current_dir(path)
    return os.listdir(current_dir / "pages")


def get_classified_pages(path):
    page_types = get_page_types(path)
    current_dir = get_current_dir(path)
    pages = {
        pt: list(
            set([int(f.split("-")[0]) for f in os.listdir(current_dir / "pages" / pt)])
        )
        for pt in page_types
    }
    tot_pages = [pn for page_numbers in pages.values() for pn in page_numbers]
    if tot_pages != list(set(tot_pages)):
        raise Exception("Some page is classified in two different ways")
    return pages


def get_pages_with_type(path):
    classified_pages = get_classified_pages(path)
    return [(p, pt) for pt, pages in classified_pages.items() for p in pages]


def get_expected_pdf_blocks(path, page_types):
    current_dir = get_current_dir(path)
    reference_pdf_blks = {}
    # a = get_algorithm(path)
    # doc = get_pdf_document(path)
    # for page, pages_type in page_types:
    #     pdf_blks = apply_pdf_extract(a,doc,page,pages_type)
    #     with open(current_dir / "pages" / pages_type / f"{page}-pdf_blks.pkl", "wb") as f:
    #         dill.dump(pdf_blks, f)
    for page, page_type in page_types:
        with (current_dir / "pages" / page_type / f"{page}-pdf_blks.pkl").open(
            "rb"
        ) as f:
            reference_pdf_blks[page] = dill.load(f)

    return reference_pdf_blks


def get_expected_text_blocks(path, page_types):
    current_dir = get_current_dir(path)
    reference_txt_blks = {}
    # a = get_algorithm(path)
    # pdf_blks = get_expected_pdf_blocks(path,page_types)
    # for page, pages_type in page_types:
    #     txt_blks = apply_text_filter(
    #         a,pdf_blks[page],test_companies,pages_type
    #     )
    #     with open(current_dir / "pages" / pages_type / f"{page}-txt_blks.pkl", "wb") as f:
    #         dill.dump(txt_blks, f)
    for page, page_type in page_types:
        with (current_dir / "pages" / page_type / f"{page}-txt_blks.pkl").open(
            "rb"
        ) as f:
            reference_txt_blks[page] = dill.load(f)
    return reference_txt_blks


def get_expected_results(path, page_types):
    current_dir = get_current_dir(path)
    reference_results = {}
    # a = get_algorithm(path)
    # doc = get_pdf_document(path)
    # txt_blks = get_expected_text_blocks(path,page_types)
    # for page, pages_type in page_types:
    #     results = apply_deserialize(a,txt_blks[page],pages_type)
    #     with open(current_dir / "pages" / pages_type / f"{page}-results.pkl", "wb") as f:
    #         dill.dump(results, f)
    for page, page_type in page_types:
        with (current_dir / "pages" / page_type / f"{page}-results.pkl").open(
            "rb"
        ) as f:
            reference_results[page] = dill.load(f)
    return reference_results


def apply_pdf_extract(algorithm, doc, page, page_type):
    page_content = doc[page - 1]
    return algorithm.apply_pdf_extract(page_content, page_type)


def apply_text_filter(algorithm, pdf_blks, filter_data, page_type):
    return algorithm.apply_text_filter(pdf_blks, filter_data, page_type)


def apply_deserialize(algorithm, txt_blks, page_type):
    return algorithm.apply_deserialize(txt_blks, page_type)


def get_output(path):
    current_dir = get_current_dir(path)
    reference_csv = pd.read_csv(
        current_dir / "out" / "investments.csv", index_col=False
    )
    reference_dict = yaml.safe_load(
        (current_dir / "out" / "investments_add_infos.yaml").open("r")
    )
    reference_log = pd.read_csv(
        current_dir / "out" / ".log.csv", index_col=False, encoding="utf-8"
    )
    return {
        "investments": reference_csv,
        "investments additional infos": reference_dict,
        "log": reference_log,
    }


def run_alghoritm(path):
    current_dir = get_current_dir(path)
    conf["PDF"] = current_dir / "report.pdf"
    fmt = get_format_name(path)
    conf["FORMAT"] = fmt
    out_name = fmt if fmt == current_dir.name else f"{fmt}-{current_dir.name}"
    conf["OUT_PATH"] = out_dir / out_name
    fra.main.main(conf)
    out_csv = pd.read_csv(conf["OUT_PATH"] / "investments.csv", index_col=False)
    out_dict = yaml.safe_load(
        (conf["OUT_PATH"] / "investments_add_infos.yaml").open("r")
    )
    log = pd.read_csv(conf["OUT_PATH"] / ".log.csv", index_col=False, encoding="utf-8")
    return {
        "investments": out_csv,
        "investments additional infos": out_dict,
        "log": log,
    }


def pytest_generate_tests(metafunc):
    """Dynamically parametrize tests based on files in the directory."""
    if "page" in metafunc.fixturenames:
        # Get the path from the marker
        path_mark = metafunc.definition.get_closest_marker("path")
        if path_mark:
            current_path = Path(path_mark.args[0])
            current_dir = current_path.parent

            # Get page types and numbers
            page_types = os.listdir(current_dir / "pages")

            # Collect all page numbers
            page_numbers = []
            for pt in page_types:
                pages_dir = current_dir / "pages" / pt
                if pages_dir.exists():
                    numbers = [int(f.split("-")[0]) for f in os.listdir(pages_dir)]
                    page_numbers.extend(numbers)

            # Remove duplicates and parametrize
            page_numbers = list(set(page_numbers))
            metafunc.parametrize("page", page_numbers)


@pytest.fixture(scope="session")
def test_targets():
    df = get_target_companies(["TEST"])
    return freeports_lib.text_filter.matcher.CompanyMatchInfos.compile_from_pandas_df(
        df
    )


@pytest.fixture
def current_dir(request):
    path_mark = request.node.get_closest_marker("path")
    current_path = Path(path_mark.args[0])
    current_dir = current_path.parent
    return current_dir


@pytest.fixture
def pdf_document(current_dir):
    pdf = Document(current_dir / "report.pdf")
    pages = [p.get_text("dict") for p in pdf]
    return pages


@pytest.fixture
def format_name(current_dir):
    fmt = os.path.split(current_dir)[-1]
    if fmt not in VALID_FORMATS:
        fmt = os.path.split(current_dir.parent)[-1]
    return fmt


@pytest.fixture
def algorithm(format_name):
    return Algorithm.load(format_name)


@pytest.fixture
def page_types(current_dir):
    return os.listdir(current_dir / "pages")


@pytest.fixture
def classified_pages(page_types, current_dir):
    pages = {
        pt: list(
            set([int(f.split("-")[0]) for f in os.listdir(current_dir / "pages" / pt)])
        )
        for pt in page_types
    }
    tot_pages = [pn for page_numbers in pages.values() for pn in page_numbers]
    if tot_pages != list(set(tot_pages)):
        raise Exception("Some page is classified in two different ways")
    return pages


@pytest.fixture
def expected_page_type(classified_pages):
    def compute_expected_page_type(page_number):
        for pt, pages in classified_pages.items():
            if page_number in pages:
                return pt

    return compute_expected_page_type


@pytest.fixture
def expected_pdf_blocks(expected_page_type, current_dir):
    def compute_expected_pdf_blocks(page):
        reference_pdf_blks = None
        with (
            current_dir / "pages" / expected_page_type(page) / f"{page}-pdf_blks.pkl"
        ).open("rb") as f:
            reference_pdf_blks = dill.load(f)
        return reference_pdf_blks

    return compute_expected_pdf_blocks


@pytest.fixture
def expected_text_blocks(expected_page_type, current_dir):
    def compute_expected_text_blocks(page):
        reference_txt_blks = None
        with (
            current_dir / "pages" / expected_page_type(page) / f"{page}-txt_blks.pkl"
        ).open("rb") as f:
            reference_txt_blks = dill.load(f)
        return reference_txt_blks

    return compute_expected_text_blocks


@pytest.fixture
def expected_results(expected_page_type, current_dir):
    def compute_expected_results(page):
        reference_txt_blks = None
        with (
            current_dir / "pages" / expected_page_type(page) / f"{page}-results.pkl"
        ).open("rb") as f:
            reference_txt_blks = dill.load(f)
        return reference_txt_blks

    return compute_expected_results


@pytest.fixture
def pdf_extract(algorithm, expected_page_type, pdf_document):
    def compute_pdf_extract(page):
        page_content = pdf_document[page - 1]
        return algorithm.apply_pdf_extract(page_content, expected_page_type(page))

    return compute_pdf_extract


@pytest.fixture
def text_filter(algorithm, expected_pdf_blocks, expected_page_type, pdf_document):
    def compute_text_filter(page, filter_data):
        return algorithm.apply_text_filter(
            expected_pdf_blocks(page), filter_data, expected_page_type(page)
        )

    return compute_text_filter


@pytest.fixture
def deserialize(algorithm, expected_text_blocks, expected_page_type, pdf_document):
    def compute_deserialize(page):
        return algorithm.apply_deserialize(
            expected_text_blocks(page), expected_page_type(page)
        )

    return compute_deserialize
