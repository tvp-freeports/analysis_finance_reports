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


# def get_segment(fmt, pipeline_name, segment_index):
#     print(get_pipelines(fmt))
#     return [
#         seg for seg in get_pipelines(fmt)[pipeline_name]
#     ][segment_index]


# def get_fmt_pipeline_name(path):
#     current_path = Path(path)
#     current_dir = current_path.parent
#     file_name = current_path.stem
#     fmt = os.path.split(current_dir)[-1]
#     if fmt not in VALID_FORMATS:
#         fmt = os.path.split(current_dir.parent)[-1]
#     pipeline_name = ""
#     name_test = "test"
#     if file_name != name_test:
#         pipeline_name = file_name.replace(f"{name_test}_", "")

#     return fmt, pipeline_name


# def get_pages(path,classified=False):
#     current_path = Path(path)
#     current_dir = current_path.parent
#     page_types = get_page_types(path)
#     pages = {
#         pt: list(set([
#             int(f.split("-")[0]) for f in os.listdir(current_dir / "pages" / pt )
#         ])) for pt in page_types
#     }
#     tot_pages=[
#         pn for page_numbers in pages.values() for pn in page_numbers
#     ]
#     if tot_pages!=list(set(tot_pages)):
#         raise Exception("Some page is classified in two different ways")
#     tot_pages=set(tot_pages)
#     if classified:
#         return pages
#     else:
#         return tot_pages

# def get_page_types(path):
#     current_path = Path(path)
#     current_dir = current_path.parent
#     return os.listdir(current_dir / "pages" )


# def generic_test_page_classification(page, path):
#     current_path = Path(path)
#     current_dir = current_path.parent
#     fmt, _ = get_fmt_pipeline_name(path)
#     a=Algorithm.load(fmt)
#     pdf = Document(current_dir / "report.pdf")
#     pages = [p.get_text("dict") for p in pdf]
#     stored_pages = get_pages(path,classified=True)
#     page_type=a.classify_page(pages,page)
#     for pt,page_numbers in stored_pages.items():
#         if page in page_numbers:
#             assert pt == page_type


# def generic_test_pdf_filter(page, path):
#     current_path = Path(path)
#     current_dir = current_path.parent
#     pdf = Document(current_dir / "report.pdf")
#     page_dict = pdf[page - 1].get_text("dict")
#     fmt, pipeline_name = get_fmt_pipeline_name(path)
#     pdf_filter = get_segment(fmt, pipeline_name, 0)
#     pdf_blks = pdf_filter(page_dict)
#     # dill.dump(pdf_blks,(current_dir / "pages" / f"{page}-pdf_blks.pkl").open("wb"))
#     reference_pdf_blks = None
#     with (current_dir / "pages" / f"{page}-pdf_blks.pkl").open("rb") as f:
#         reference_pdf_blks = dill.load(f)

#     assert pdf_blks == reference_pdf_blks


# def generic_test_text_extract(page, path):
#     current_path = Path(path)
#     current_dir = current_path.parent
#     pdf_blks = None
#     with (current_dir / "pages" / f"{page}-pdf_blks.pkl").open("rb") as f:
#         pdf_blks = dill.load(f)
#     fmt, pipeline_name = get_fmt_pipeline_name(path)
#     text_extract = get_segment(fmt, pipeline_name, 1)
#     trgs=None
#     trgs = freeports_lib.text_extract.matcher.CompanyMatchInfos.compile_from_pandas_df(targets)
#     txt_blks = text_extract(pdf_blks, trgs)

#     # dill.dump(txt_blks,(current_dir / "pages" / f"{page}-txt_blks.pkl").open("wb"))
#     reference_txt_blks = None
#     with (current_dir / "pages" / f"{page}-txt_blks.pkl").open("rb") as f:
#         reference_txt_blks = dill.load(f)

#     assert txt_blks == reference_txt_blks


# def generic_test_deserialize(page, path):
#     txt_blks = None
#     current_path = Path(path)
#     current_dir = current_path.parent
#     with (current_dir / "pages" / f"{page}-txt_blks.pkl").open("rb") as f:
#         txt_blks = dill.load(f)
#     fmt, pipeline_name = get_fmt_pipeline_name(path)
#     deserialize = get_segment(fmt, pipeline_name, 2)
#     results = [
#         deserialize(txt_blks) for txt_blk in txt_blks
#     ]
#     # dill.dump(results,(current_dir / "pages" / f"{page}-results.pkl").open("wb"))
#     reference_results = None
#     with (current_dir / "pages" / f"{page}-results.pkl").open("rb") as f:
#         reference_results = dill.load(f)

#     assert results == reference_results


# def generic_test_pipelines(path):
#     current_path = Path(path)
#     current_dir = current_path.parent
#     conf["PDF"] = current_dir / "report.pdf"
#     fmt = get_fmt_pipeline_name(path)[0]
#     conf["FORMAT"] = fmt
#     out_name = fmt if fmt == current_dir.name else f"{fmt}-{current_dir.name}"
#     conf["OUT_PATH"] = out_dir / out_name
#     fra.main.main(conf)
#     out_csv = pd.read_csv(conf["OUT_PATH"] / "investments.csv", index_col=False)
#     reference_csv = pd.read_csv(
#         current_dir / "out" / "investments.csv", index_col=False
#     )
#     pd.testing.assert_frame_equal(
#         out_csv.sort_values(by=out_csv.columns.tolist()).reset_index(drop=True),
#         reference_csv.sort_values(by=reference_csv.columns.tolist()).reset_index(
#             drop=True
#         ),
#     )
#     out_dict = yaml.safe_load(
#         (conf["OUT_PATH"] / "investments_add_infos.yaml").open("r")
#     )
#     reference_dict = yaml.safe_load(
#         (current_dir / "out" / "investments_add_infos.yaml").open("r")
#     )
#     assert out_dict == reference_dict

#     log = pd.read_csv(conf["OUT_PATH"] / ".log.csv", index_col=False,encoding="utf-8")
#     reference_log = pd.read_csv(current_dir / "out" / ".log.csv", index_col=False,encoding="utf-8")
#     pd.testing.assert_frame_equal(
#         log.sort_values(by=log.columns.tolist()).reset_index(drop=True),
#         reference_log.sort_values(by=reference_log.columns.tolist()).reset_index(
#             drop=True
#         ),
#     )


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
    return freeports_lib.text_extract.matcher.CompanyMatchInfos.compile_from_pandas_df(
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
