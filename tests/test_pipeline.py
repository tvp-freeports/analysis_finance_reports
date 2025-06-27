from conftest import data_dir, all_pages_tests, out_dir
import pytest
import importlib
import dill
import freeports_analysis as fra
import pandas as pd


@pytest.fixture
def conf():
    return {
        "VERBOSITY": 2,
        "BATCH_WORKERS": None,
        "BATCH": None,
        "OUT_CSV": None,
        "SAVE_PDF": False,
        "URL": None,
        "PDF": None,
        "FORMAT": None,
        "CONFIG_FILE": None,
    }


@pytest.mark.integration_tests
@pytest.mark.parametrize("fmt", all_pages_tests)
def test_csv_output(fmt, conf):
    conf["PDF"] = data_dir / fmt / "report.pdf"
    conf["FORMAT"] = fra.consts.PdfFormats.__members__[fmt]
    conf["OUT_CSV"] = out_dir / f"out-{fmt}.csv"
    fra.main.main(conf)
    out_csv = pd.read_csv(conf["OUT_CSV"])
    reference_csv = pd.read_csv(data_dir / fmt / "out.csv")

    assert out_csv.equals(reference_csv)
