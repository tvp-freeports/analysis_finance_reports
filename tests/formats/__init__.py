from pymupdf import Document
from pathlib import Path
import os
import dill
import pandas as pd
from lxml import etree
import yaml
import freeports_analysis as fra
from freeports_analysis.formats.data import VALID_FORMATS
from freeports_analysis.formats.algorithms import get_pipelines
from freeports_analysis.formats.utils.text_extract.match import dataframe_to_match
from ..conftest import out_dir, xml_parser, targets, conf


def get_segment(fmt, pipeline_name, segment_index):
    return get_pipelines(fmt)[pipeline_name][segment_index]


def get_fmt_pipeline_name(path):
    current_path = Path(path)
    current_dir = current_path.parent
    file_name = current_path.stem
    fmt = os.path.split(current_dir)[-1]
    report_id = None
    if fmt not in VALID_FORMATS:
        report_id = fmt
        fmt = os.path.split(current_dir.parent)[-1]
    pipeline_name = ""
    fmt_suffix = fmt.lower().replace("-", "_").replace(".", "_")
    name_test = (
        f"test_{fmt_suffix}" if report_id is None else f"test_{fmt_suffix}_{report_id}"
    )
    if file_name != name_test:
        pipeline_name = file_name.replace(f"{name_test}_", "")

    return fmt, pipeline_name


def get_pages(path):
    current_path = Path(path)
    current_dir = current_path.parent
    pages = list(set([int(f.split("-")[0]) for f in os.listdir(current_dir / "pages")]))
    return pages


def generic_test_pdf_filter(page, path):
    current_path = Path(path)
    current_dir = current_path.parent
    pdf = Document(current_dir / "report.pdf")
    xml_str = pdf[page - 1].get_text("xml")
    xml_tree = etree.fromstring(xml_str.encode(), parser=xml_parser)
    fmt, pipeline_name = get_fmt_pipeline_name(path)
    pdf_filters = get_segment(fmt, pipeline_name, 0)
    pdf_blks = [blk for pdf_filter in pdf_filters for blk in pdf_filter(xml_tree)]
    # dill.dump(pdf_blks,(current_dir / "pages" / f"{page}-pdf_blks.pkl").open("wb"))
    reference_pdf_blks = None
    with (current_dir / "pages" / f"{page}-pdf_blks.pkl").open("rb") as f:
        reference_pdf_blks = dill.load(f)

    assert pdf_blks == reference_pdf_blks


def generic_test_text_extract(page, path):
    current_path = Path(path)
    current_dir = current_path.parent
    pdf_blks = None
    with (current_dir / "pages" / f"{page}-pdf_blks.pkl").open("rb") as f:
        pdf_blks = dill.load(f)
    fmt, pipeline_name = get_fmt_pipeline_name(path)
    text_extracts = get_segment(fmt, pipeline_name, 1)
    trgs = dataframe_to_match(targets)
    txt_blks = [
        blk for text_extract in text_extracts for blk in text_extract(pdf_blks, trgs)
    ]
    # dill.dump(txt_blks,(current_dir / "pages" / f"{page}-txt_blks.pkl").open("wb"))
    reference_txt_blks = None
    with (current_dir / "pages" / f"{page}-txt_blks.pkl").open("rb") as f:
        reference_txt_blks = dill.load(f)

    assert txt_blks == reference_txt_blks


def generic_test_deserialize(page, path):
    txt_blks = None
    current_path = Path(path)
    current_dir = current_path.parent
    with (current_dir / "pages" / f"{page}-txt_blks.pkl").open("rb") as f:
        txt_blks = dill.load(f)
    fmt, pipeline_name = get_fmt_pipeline_name(path)
    deserializes = get_segment(fmt, pipeline_name, 2)
    results = [
        deserialize(txt_blk) for deserialize in deserializes for txt_blk in txt_blks
    ]
    # dill.dump(results,(current_dir / "pages" / f"{page}-results.pkl").open("wb"))
    reference_results = None
    with (current_dir / "pages" / f"{page}-results.pkl").open("rb") as f:
        reference_results = dill.load(f)

    assert results == reference_results


def generic_test_pipelines(path):
    current_path = Path(path)
    current_dir = current_path.parent
    conf["PDF"] = current_dir / "report.pdf"
    fmt = get_fmt_pipeline_name(path)[0]
    conf["FORMAT"] = fmt
    out_name = fmt if fmt == current_dir.name else f"{fmt}-{current_dir.name}"
    conf["OUT_PATH"] = out_dir / out_name
    fra.main.main(conf)
    out_csv = pd.read_csv(conf["OUT_PATH"] / "investments.csv", index_col=False)
    reference_csv = pd.read_csv(
        current_dir / "out" / "investments.csv", index_col=False
    )
    pd.testing.assert_frame_equal(
        out_csv.sort_values(by=out_csv.columns.tolist()).reset_index(drop=True),
        reference_csv.sort_values(by=reference_csv.columns.tolist()).reset_index(
            drop=True
        ),
    )
    out_dict = yaml.safe_load(
        (conf["OUT_PATH"] / "investments_add_infos.yaml").open("r")
    )
    reference_dict = yaml.safe_load(
        (current_dir / "out" / "investments_add_infos.yaml").open("r")
    )
    assert out_dict == reference_dict
