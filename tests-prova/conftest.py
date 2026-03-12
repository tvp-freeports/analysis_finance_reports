import pytest
import os
from pathlib import Path
import dill
import pandas as pd
import yaml
from pytest import Collector, Item
from pymupdf import Document
from freeports_analysis.formats.data import VALID_FORMATS
from freeports_analysis.formats.algorithms import Algorithm
from freeports_analysis.data import get_target_companies
from freeports_analysis.main import main as run_analysis
from freeports_analysis.conf_parse import (
    OutStructureNormalMode,
    OutFlagsNormalMode,
    FreeportsFileConfig,
)
import freeports_lib

test_companies_df = get_target_companies(["TEST"])
test_companies = (
    freeports_lib.text_filter.matcher.CompanyMatchInfos.compile_from_pandas_df(
        test_companies_df
    )
)


class PdfExtractTest(Item):
    def __init__(self, name, parent, page_num, page_type, format_name):
        super().__init__(name=name, parent=parent)
        self.page_num = page_num
        self.page_type = page_type
        self.format_name = format_name

    def runtest(self):
        algorithm = Algorithm.load(self.format_name)
        doc = self.parent.get_pdf_document()

        # Get expected blocks
        expected_path = (
            self.parent.path
            / self.format_name
            / "pages"
            / self.page_type
            / f"{self.page_num}-pdf_blks.pkl"
        )
        with open(expected_path, "rb") as f:
            expected = dill.load(f)

        # Run test
        page_content = doc[self.page_num - 1].get_text("dict")
        result = algorithm.apply_pdf_extract(page_content, self.page_type)

        assert result == expected, (
            f"PDF extract failed for page {self.page_num} ({self.page_type})"
        )


class TextFilterTest(Item):
    def __init__(self, name, parent, page_num, page_type, format_name):
        super().__init__(name=name, parent=parent)
        self.page_num = page_num
        self.page_type = page_type
        self.format_name = format_name

    def runtest(self):
        algorithm = Algorithm.load(self.format_name)

        # Load PDF blocks
        pdf_path = (
            self.parent.path
            / self.format_name
            / "pages"
            / self.page_type
            / f"{self.page_num}-pdf_blks.pkl"
        )
        with open(pdf_path, "rb") as f:
            pdf_blks = dill.load(f)

        # Get expected text blocks
        expected_path = (
            self.parent.path
            / self.format_name
            / "pages"
            / self.page_type
            / f"{self.page_num}-txt_blks.pkl"
        )
        with open(expected_path, "rb") as f:
            expected = dill.load(f)

        # Run test
        result = algorithm.apply_text_filter(pdf_blks, test_companies, self.page_type)

        assert result == expected, (
            f"Text filter failed for page {self.page_num} ({self.page_type})"
        )


class DeserializeTest(Item):
    def __init__(self, name, parent, page_num, page_type, format_name):
        super().__init__(name=name, parent=parent)
        self.page_num = page_num
        self.page_type = page_type
        self.format_name = format_name

    def runtest(self):
        algorithm = Algorithm.load(self.format_name)

        # Load text blocks
        txt_path = (
            self.parent.path
            / self.format_name
            / "pages"
            / self.page_type
            / f"{self.page_num}-txt_blks.pkl"
        )
        with open(txt_path, "rb") as f:
            txt_blks = dill.load(f)

        # Get expected results
        expected_path = (
            self.parent.path
            / self.format_name
            / "pages"
            / self.page_type
            / f"{self.page_num}-results.pkl"
        )
        with open(expected_path, "rb") as f:
            expected = dill.load(f)

        # Run test
        result = algorithm.apply_deserialize(txt_blks, self.page_type)

        assert result == expected, (
            f"Deserialize failed for page {self.page_num} ({self.page_type})"
        )


class PipelineTest(Item):
    def __init__(self, name, parent, format_name):
        super().__init__(name=name, parent=parent)
        self.format_name = format_name

    def runtest(self):
        # Run the full pipeline
        config = {
            "PDF": self.parent.path / self.format_name / "report.pdf",
            "FORMAT": self.format_name,
            "OUT_PATH": self.parent.path / self.format_name / "out_test",
            "VERBOSITY": 2,
            "N_WORKERS": 1,
            "BATCH_FILE": None,
            "SAVE_PDF": False,
            "URL": None,
            "CONFIG_FILE": FreeportsFileConfig.find_config(),
            "PREFIX_OUT": None,
            "TARGET_LISTS": ["TEST"],
            "OUT_PROFILE": OutStructureNormalMode.REGULAR,
            "OUT_FLAGS": OutFlagsNormalMode(0),
        }

        # Ensure output directory exists and is clean
        out_path = config["OUT_PATH"]
        if out_path.exists():
            import shutil

            shutil.rmtree(out_path)
        out_path.mkdir(parents=True)

        # Run analysis
        run_analysis(config)

        # Get actual results
        actual_csv = pd.read_csv(out_path / "investments.csv", index_col=False)
        actual_dict = yaml.safe_load(
            (out_path / "investments_add_infos.yaml").open("r")
        )
        actual_log = pd.read_csv(
            out_path / ".log.csv", index_col=False, encoding="utf-8"
        )

        # Get expected results
        expected_csv = pd.read_csv(
            self.parent.path / self.format_name / "out" / "investments.csv",
            index_col=False,
        )
        expected_dict = yaml.safe_load(
            (
                self.parent.path
                / self.format_name
                / "out"
                / "investments_add_infos.yaml"
            ).open("r")
        )
        expected_log = pd.read_csv(
            self.parent.path / self.format_name / "out" / ".log.csv",
            index_col=False,
            encoding="utf-8",
        )

        # Assertions
        pd.testing.assert_frame_equal(
            actual_csv.sort_values(by=actual_csv.columns.tolist()).reset_index(
                drop=True
            ),
            expected_csv.sort_values(by=expected_csv.columns.tolist()).reset_index(
                drop=True
            ),
            obj="investments.csv",
        )

        assert actual_dict == expected_dict, "investments_add_infos.yaml mismatch"

        pd.testing.assert_frame_equal(
            actual_log.sort_values(by=actual_log.columns.tolist()).reset_index(
                drop=True
            ),
            expected_log.sort_values(by=expected_log.columns.tolist()).reset_index(
                drop=True
            ),
            obj=".log.csv",
        )


class FreeportsFormat(Collector):
    def __init__(self, name, parent, format_name, path):
        super().__init__(name=name, parent=parent)
        self.format_name = format_name
        self.path = Path(path)
        self.pdf_document = None

    def get_pdf_document(self):
        if self.pdf_document is None:
            pdf_path = self.path / self.format_name / "report.pdf"
            self.pdf_document = Document(pdf_path)
        return self.pdf_document

    def collect(self):
        directory = self.path / self.format_name

        # Validate directory structure and collect page information
        pdf_blks = set()
        txt_blks = set()
        results = set()
        pages_by_type = {}
        all_pages = set()

        # Check for required directories/files
        has_report = (directory / "report.pdf").exists()
        has_out = (directory / "out").exists()

        if not has_report:
            raise pytest.CollectError(f"Missing report.pdf in {directory}")

        pages_dir = directory / "pages"
        if not pages_dir.exists():
            raise pytest.CollectError(f"Missing pages directory in {directory}")

        # Scan pages directory
        for page_type in os.listdir(pages_dir):
            type_dir = pages_dir / page_type
            if not type_dir.is_dir():
                continue

            pages_by_type[page_type] = set()

            for f in os.listdir(type_dir):
                if "-" not in f:
                    continue

                page_num_str, file_type = f.split("-", 1)
                try:
                    page_num = int(page_num_str)
                except ValueError:
                    continue

                pages_by_type[page_type].add(page_num)
                all_pages.add(page_num)

                if file_type == "pdf_blks.pkl":
                    pdf_blks.add(page_num)
                elif file_type == "txt_blks.pkl":
                    txt_blks.add(page_num)
                elif file_type == "results.pkl":
                    results.add(page_num)
                else:
                    raise pytest.CollectError(f"Unknown file in pages folder: {f}")

        # Validate that pages are uniquely classified
        total_pages = []
        for pages in pages_by_type.values():
            total_pages.extend(list(pages))
        if len(total_pages) != len(set(total_pages)):
            raise pytest.CollectError("Found pages classified in multiple ways")

        # Determine which tests can run
        pdf_extract_enabled = set()
        text_filter_enabled = set()
        deserialize_enabled = set()

        for page in all_pages:
            if has_report and page in pdf_blks:
                pdf_extract_enabled.add(page)
            if page in pdf_blks and page in txt_blks:
                text_filter_enabled.add(page)
            if page in txt_blks and page in results:
                deserialize_enabled.add(page)

        pipeline_enabled = has_report and has_out

        # Generate tests
        for page_type, pages in pages_by_type.items():
            for page in pages:
                if page in pdf_extract_enabled:
                    yield PdfExtractTest.from_parent(
                        parent=self,
                        name=f"test_pdf_extract[{page_type}:{page}]",
                        page_num=page,
                        page_type=page_type,
                        format_name=self.format_name,
                    )

                if page in text_filter_enabled:
                    yield TextFilterTest.from_parent(
                        parent=self,
                        name=f"test_text_filter[{page_type}:{page}]",
                        page_num=page,
                        page_type=page_type,
                        format_name=self.format_name,
                    )

                if page in deserialize_enabled:
                    yield DeserializeTest.from_parent(
                        parent=self,
                        name=f"test_deserialize[{page_type}:{page}]",
                        page_num=page,
                        page_type=page_type,
                        format_name=self.format_name,
                    )

        if pipeline_enabled:
            yield PipelineTest.from_parent(
                parent=self, name="test_pipeline", format_name=self.format_name
            )


@pytest.hookimpl
def pytest_collect_directory(path, parent):
    """Collect directories that match valid formats."""
    path = Path(path)
    dirname = path.name

    if dirname in VALID_FORMATS:
        return FreeportsFormat.from_parent(
            parent=parent,
            name=f"FreeportsFormat[{dirname}]",
            format_name=dirname,
            path=path.parent,  # Store parent path to access the format directory
        )

    # Recurse into subdirectories
    return None


def pytest_collect_file(file_path, parent):
    """Skip collecting regular test files in format directories."""
    file_path = Path(file_path)

    # Check if this file is inside a format directory
    for part in file_path.parts:
        if part in VALID_FORMATS:
            # Skip collecting this file - our collector handles it
            return None

    # Let pytest handle other files normally
    return None
