import importlib
from pymupdf import Document
import dill
import pandas as pd
from lxml import etree
import freeports_analysis as fra
from ..conftest import data_dir, out_dir, xml_parser, targets, conf


def generic_test_pdf_filter(fmt, page):
    pdf = Document(data_dir / fmt / "report.pdf")
    xml_str = pdf[page].get_text("xml")
    xml_tree = etree.fromstring(xml_str.encode(), parser=xml_parser)
    module = importlib.import_module(f"freeports_analysis.formats.{fmt.lower()}")
    pdf_blks = module.pdf_filter(xml_tree, page)
    # with (data_dir / fmt / f"pdf_blks-{page}.pkl").open("wb") as f:
    #     dill.dump(pdf_blks,f)
    reference_pdf_blks = None
    with (data_dir / fmt / f"pdf_blks-{page}.pkl").open("rb") as f:
        reference_pdf_blks = dill.load(f)

    assert pdf_blks == reference_pdf_blks


def generic_test_text_extract(fmt, page):
    pdf_blks = None
    with (data_dir / fmt / f"pdf_blks-{page}.pkl").open("rb") as f:
        pdf_blks = dill.load(f)

    module = importlib.import_module(f"freeports_analysis.formats.{fmt.lower()}")
    txt_blks = module.text_extract(pdf_blks, targets)
    # with (data_dir / fmt / f"txt_blks-{page}.pkl").open("wb") as f:
    #     dill.dump(txt_blks,f)
    reference_txt_blks = None
    with (data_dir / fmt / f"txt_blks-{page}.pkl").open("rb") as f:
        reference_txt_blks = dill.load(f)

    assert txt_blks == reference_txt_blks


def generic_test_deserialize(fmt, page):
    txt_blks = None
    with (data_dir / fmt / f"txt_blks-{page}.pkl").open("rb") as f:
        txt_blks = dill.load(f)

    module = importlib.import_module(f"freeports_analysis.formats.{fmt.lower()}")
    financial_data = [module.deserialize(blk, targets) for blk in txt_blks]
    # with (data_dir / fmt / f"txt_blks-{page}.pkl").open("wb") as f:
    #     dill.dump(txt_blks,f)
    reference_financial_data = None
    with (data_dir / fmt / f"financial_data-{page}.pkl").open("rb") as f:
        reference_financial_data = dill.load(f)

    assert financial_data == reference_financial_data


def generic_test_pipeline(fmt):
    conf["PDF"] = data_dir / fmt / "report.pdf"
    conf["FORMAT"] = fra.consts.PdfFormats.__members__[fmt]
    conf["OUT_CSV"] = out_dir / f"out-{fmt}.csv"
    fra.main.main(conf)
    out_csv = pd.read_csv(conf["OUT_CSV"], index_col=False)
    reference_csv = pd.read_csv(data_dir / fmt / "out.csv", index_col=False)
    pd.testing.assert_frame_equal(
        out_csv.sort_values(by=out_csv.columns.tolist()).reset_index(drop=True),
        reference_csv.sort_values(by=reference_csv.columns.tolist()).reset_index(
            drop=True
        ),
    )
