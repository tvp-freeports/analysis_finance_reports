import pytest
import os
from pathlib import Path
import dill
import pandas as pd
import yaml
from pytest import Collector, Function, Item, Directory
from pymupdf import Document
from abc import ABC, abstractclassmethod
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
import _pytest.fixtures as fixtures
import _pytest.tmpdir as tmpdir
from _pytest.python import PyobjMixin

test_companies_df = get_target_companies(["TEST"])
test_companies = (
    freeports_lib.text_filter.matcher.CompanyMatchInfos.compile_from_pandas_df(
        test_companies_df
    )
)


class AlgorithmCache:
    def __init__(self):
        self.algorithms = {}

    def load(self, format_name):
        if format_name not in self.algorithms:
            self.algorithms[format_name] = Algorithm.load(format_name)
        return self.algorithms[format_name]


class FileCache(ABC):
    def __init__(self):
        self.files = {}

    @abstractclassmethod
    def load_content(self, path):
        pass

    def get_file(self, path):
        if path not in self.files:
            self.files[path] = self.load_content(path)
        return self.files[path]


class CsvCache(FileCache):
    def load_content(self, path):
        return pd.read_csv(path, index_col=False, encoding="utf-8")


class YamlCache(FileCache):
    def load_content(self, path):
        return yaml.safe_load(path.open("r"))


class PklCache(FileCache):
    def load_content(self, path):
        return dill.load(path.open("rb"))


class PdfCache(FileCache):
    def load_content(self, path):
        return Document(path)


class DocumentCache(ABC):
    def __init__(self):
        self.files = {}
        self.pages = {}

    def get_doc(self, path, pdf_cache):
        if path not in self.files:
            self.files[path] = [p.get_text("dict") for p in pdf_cache.get_file(path)]
        return self.files[path]

    def get_page(self, path, page_n, pdf_cache):
        if (path, page_n) not in self.pages:
            self.pages[(path, page_n)] = pdf_cache.get_file(path)[page_n - 1].get_text(
                "dict"
            )
        return self.pages[(path, page_n)]


class FormatCache:
    def __init__(self):
        self.algorithm = AlgorithmCache()
        self.csv = CsvCache()
        self.yaml = YamlCache()
        self.pkl = PklCache()
        self.pdf = PdfCache()
        self.doc = DocumentCache()


class PdfExtractTest(Function):
    def __init__(self, name, parent, page_num, page_type, format_name, **kwargs):
        super().__init__(name=name, parent=parent, callobj=self.runtest, **kwargs)
        self.page_num = page_num
        self.page_type = page_type
        self.format_name = format_name

    def runtest(self):
        algorithm = self.parent.cache.algorithm.load(self.format_name)
        page_content = self.parent.get_page(self.page_num)

        result = algorithm.apply_pdf_extract(page_content, self.page_type)

        expected_path = (
            self.parent.path
            / "pages"
            / self.page_type
            / f"{self.page_num}-pdf_blks.pkl"
        )

        # with open(expected_path, "wb") as f:
        #     dill.dump(result, f)

        expected = self.parent.cache.pkl.get_file(expected_path)
        print(expected)
        print(self.page_num)
        print(expected_path)
        print(result)
        assert frozenset(result) == frozenset(expected), (
            f"PDF extract failed for page {self.page_num} ({self.page_type})"
        )


class TextFilterTest(Function):
    def __init__(self, name, parent, page_num, page_type, format_name, **kwargs):
        super().__init__(name=name, parent=parent, callobj=self.runtest, **kwargs)
        self.page_num = page_num
        self.page_type = page_type
        self.format_name = format_name
        self.is_filter_data = (
            self.parent.path / "pages" / self.page_type / "filter_data.pkl"
        ).exists()

    def runtest(self):
        algorithm = self.parent.cache.algorithm.load(self.format_name)

        filter_data = test_companies
        if self.is_filter_data:
            filter_data = self.parent.cache.pkl.get_file(
                self.parent.path / "pages" / self.page_type / "filter_data.pkl"
            )

        page_content = self.parent.get_page(self.page_num)
        # Run test
        result = algorithm.apply_text_filter(page_content, filter_data, self.page_type)

        # Get expected text blocks
        expected_path = (
            self.parent.path
            / "pages"
            / self.page_type
            / f"{self.page_num}-txt_blks.pkl"
        )

        # with open(expected_path, "wb") as f:
        #     dill.dump(result, f)

        expected = self.parent.cache.pkl.get_file(expected_path)
        assert frozenset(result) == frozenset(expected), (
            f"Text filter failed for page {self.page_num} ({self.page_type})"
        )


class DeserializeTest(Function):
    def __init__(self, name, parent, page_num, page_type, format_name, **kwargs):
        super().__init__(name=name, parent=parent, callobj=self.runtest, **kwargs)
        self.page_num = page_num
        self.page_type = page_type
        self.format_name = format_name
        self.is_filter_data = (
            self.parent.path / "pages" / self.page_type / "filter_data.pkl"
        ).exists()

    def runtest(self):
        algorithm = self.parent.cache.algorithm.load(self.format_name)
        filter_data = test_companies
        if self.is_filter_data:
            filter_data = self.parent.cache.pkl.get_file(
                self.parent.path / "pages" / self.page_type / "filter_data.pkl"
            )

        page_content = self.parent.get_page(self.page_num)

        result = algorithm.apply_deserialize(page_content, filter_data, self.page_type)

        # Get expected results
        expected_path = (
            self.parent.path / "pages" / self.page_type / f"{self.page_num}-results.pkl"
        )

        # with open(expected_path, "wb") as f:
        #     dill.dump(result, f)
        expected = self.parent.cache.pkl.get_file(expected_path)
        for i, r in enumerate(result):
            if isinstance(r, dict):
                result[i] = frozenset(r.items())
        for i, r in enumerate(expected):
            if isinstance(r, dict):
                expected[i] = frozenset(r.items())

        assert frozenset(result) == frozenset(expected), (
            f"Deserialize failed for page {self.page_num} ({self.page_type})"
        )


class PipelineTest(Function):
    def __init__(self, name, parent, format_name, document_variant, **kwargs):
        super().__init__(name=name, parent=parent, callobj=self.runtest, **kwargs)
        self.format_name = format_name
        self.document_variant = document_variant
        self._request = fixtures.TopRequest(self, _ispytest=True)

    def runtest(self):
        # Run the full pipeline
        create_dir = self._request.getfixturevalue("tmp_path_factory")
        tmp_dir = f"{self.format_name}" + (
            "" if self.document_variant is None else f"__report_{self.document_variant}"
        )
        config = {
            "PDF": self.parent.path / "report.pdf",
            "FORMAT": self.format_name,
            "OUT_PATH": create_dir.mktemp(tmp_dir, numbered=False),
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

        out_path = config["OUT_PATH"]

        # Run analysis
        run_analysis(config)

        # Get actual results
        org_actual_investments = self.parent.cache.csv.get_file(
            out_path / "investments.csv"
        )
        org_actual_add_infos = self.parent.cache.yaml.get_file(
            out_path / "investments_add_infos.yaml"
        )
        org_actual_funds = self.parent.cache.csv.get_file(out_path / "funds.csv")
        org_actual_funds_assets = self.parent.cache.csv.get_file(
            out_path / "funds_assets.csv"
        )
        org_actual_assets_managers = self.parent.cache.csv.get_file(
            out_path / "assets_managers.csv"
        )
        org_actual_inv_to_funds = self.parent.cache.csv.get_file(
            out_path / "investments_managers_to_funds.csv"
        )
        org_actual_funds_change_name = self.parent.cache.csv.get_file(
            out_path / "funds_change_name.csv"
        )
        org_actual_log = self.parent.cache.csv.get_file(out_path / ".log.csv")

        actual_investments = org_actual_investments.join(
            org_actual_funds.set_index("ID")[["Name"]].rename(
                columns={"Name": "Fund name"}
            ),
            on="Fund ID",
        ).drop(columns=["ID", "Fund ID"])
        print("ACTUAL LEGNO", len(actual_investments))
        actual_funds = org_actual_funds.join(
            org_actual_assets_managers.set_index("ID")[["Name"]].rename(
                columns={"Name": "Asset manager name"}
            ),
            on="Managment company ID",
        ).drop(columns=["ID", "Managment company ID"])

        actual_funds_assets = org_actual_funds_assets.join(
            org_actual_funds.set_index("ID")[["Name"]].rename(
                columns={"Name": "Fund name"}
            ),
            on="Fund ID",
        ).drop(columns=["Fund ID"])

        actual_inv_to_funds = (
            org_actual_inv_to_funds.join(
                org_actual_assets_managers.set_index("ID")[["Name"]].rename(
                    columns={"Name": "Asset manager name"}
                ),
                on="Investment manager ID",
            )
            .join(
                org_actual_funds.set_index("ID")[["Name"]].rename(
                    columns={"Name": "Fund name"}
                ),
                on="Fund ID",
            )
            .drop(columns=["Fund ID", "Investment manager ID"])
        )

        actual_funds_change_name = org_actual_funds_change_name.join(
            org_actual_funds.set_index("ID")[["Name"]].rename(
                columns={"Name": "Fund name"}
            ),
            on="Fund ID",
        ).drop(columns=["Fund ID"])

        actual_assets_managers = org_actual_assets_managers.drop(columns="ID")

        expected_dir = self.parent.path / "out"
        print(out_path)
        print(
            "actual",
            actual_investments.sort_values(
                by=actual_investments.columns.tolist()
            ).reset_index(drop=True),
        )

        org_expected_investments = self.parent.cache.csv.get_file(
            expected_dir / "investments.csv"
        )
        org_expected_add_infos = self.parent.cache.yaml.get_file(
            expected_dir / "investments_add_infos.yaml"
        )
        org_expected_funds = self.parent.cache.csv.get_file(expected_dir / "funds.csv")
        org_expected_funds_assets = self.parent.cache.csv.get_file(
            expected_dir / "funds_assets.csv"
        )
        org_expected_assets_managers = self.parent.cache.csv.get_file(
            expected_dir / "assets_managers.csv"
        )
        org_expected_inv_to_funds = self.parent.cache.csv.get_file(
            expected_dir / "investments_managers_to_funds.csv"
        )
        org_expected_funds_change_name = self.parent.cache.csv.get_file(
            expected_dir / "funds_change_name.csv"
        )
        org_expected_log = self.parent.cache.csv.get_file(expected_dir / ".log.csv")

        expected_investments = org_expected_investments.join(
            org_expected_funds.set_index("ID")[["Name"]].rename(
                columns={"Name": "Fund name"}
            ),
            on="Fund ID",
        ).drop(columns=["ID", "Fund ID"])
        print("LEGNO", len(expected_investments))

        expected_funds = org_expected_funds.join(
            org_expected_assets_managers.set_index("ID")[["Name"]].rename(
                columns={"Name": "Asset manager name"}
            ),
            on="Managment company ID",
        ).drop(columns=["ID", "Managment company ID"])

        expected_funds_assets = org_expected_funds_assets.join(
            org_expected_funds.set_index("ID")[["Name"]].rename(
                columns={"Name": "Fund name"}
            ),
            on="Fund ID",
        ).drop(columns=["Fund ID"])

        expected_inv_to_funds = (
            org_expected_inv_to_funds.join(
                org_expected_assets_managers.set_index("ID")[["Name"]].rename(
                    columns={"Name": "Asset manager name"}
                ),
                on="Investment manager ID",
            )
            .join(
                org_expected_funds.set_index("ID")[["Name"]].rename(
                    columns={"Name": "Fund name"}
                ),
                on="Fund ID",
            )
            .drop(columns=["Fund ID", "Investment manager ID"])
        )

        expected_funds_change_name = org_expected_funds_change_name.join(
            org_expected_funds.set_index("ID")[["Name"]].rename(
                columns={"Name": "Fund name"}
            ),
            on="Fund ID",
        ).drop(columns=["Fund ID"])

        expected_assets_managers = org_expected_assets_managers.drop(columns="ID")

        print(expected_dir)
        print(
            "expected",
            expected_investments.sort_values(
                by=expected_investments.columns.tolist()
            ).reset_index(drop=True),
        )
        # Assertions
        pd.testing.assert_frame_equal(
            actual_investments.sort_values(
                by=actual_investments.columns.tolist()
            ).reset_index(drop=True),
            expected_investments.sort_values(
                by=expected_investments.columns.tolist()
            ).reset_index(drop=True),
            obj="investments.csv",
        )
        pd.testing.assert_frame_equal(
            actual_funds.sort_values(by=actual_funds.columns.tolist()).reset_index(
                drop=True
            ),
            expected_funds.sort_values(by=expected_funds.columns.tolist()).reset_index(
                drop=True
            ),
            obj="funds.csv",
        )

        pd.testing.assert_frame_equal(
            actual_funds_assets.sort_values(
                by=actual_funds_assets.columns.tolist()
            ).reset_index(drop=True),
            expected_funds_assets.sort_values(
                by=expected_funds_assets.columns.tolist()
            ).reset_index(drop=True),
            obj="funds_assets.csv",
        )
        pd.testing.assert_frame_equal(
            actual_assets_managers.sort_values(
                by=actual_assets_managers.columns.tolist()
            ).reset_index(drop=True),
            expected_assets_managers.sort_values(
                by=expected_assets_managers.columns.tolist()
            ).reset_index(drop=True),
            obj="assets_managers.csv",
        )
        pd.testing.assert_frame_equal(
            actual_inv_to_funds.sort_values(
                by=actual_inv_to_funds.columns.tolist()
            ).reset_index(drop=True),
            expected_inv_to_funds.sort_values(
                by=expected_inv_to_funds.columns.tolist()
            ).reset_index(drop=True),
            obj="investments_managers_to_funds.csv",
        )
        pd.testing.assert_frame_equal(
            actual_funds_change_name.sort_values(
                by=actual_funds_change_name.columns.tolist()
            ).reset_index(drop=True),
            expected_funds_change_name.sort_values(
                by=expected_funds_change_name.columns.tolist()
            ).reset_index(drop=True),
            obj="funds_change_name.csv",
        )
        assert org_actual_add_infos == org_expected_add_infos, (
            "investments_add_infos.yaml mismatch"
        )

        pd.testing.assert_frame_equal(
            org_actual_log.sort_values(by=org_actual_log.columns.tolist()).reset_index(
                drop=True
            ),
            org_expected_log.sort_values(
                by=org_expected_log.columns.tolist()
            ).reset_index(drop=True),
            obj=".log.csv",
        )


class ReportVariant(Collector):
    def __init__(self, name, document_variant, **kwargs):
        super().__init__(name=name, **kwargs)
        self.document_variant = document_variant
        self.format_name = self.parent.format_name
        self.cache = FormatCache()

    def get_document(self):
        return self.cache.doc.get_doc(self.path / "report.pdf", self.cache.pdf)

    def get_page(self, page_n):
        return self.cache.doc.get_page(self.path / "report.pdf", page_n, self.cache.pdf)

    def collect(self):
        directory = self.path

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
            raise self.CollectError(f"Missing report.pdf in {directory}")

        pages_dir = directory / "pages"
        if not pages_dir.exists():
            raise self.CollectError(f"Missing pages directory in {directory}")

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
                    raise Exception(f"Unknown file in pages folder: {f}")

        # Validate that pages are uniquely classified
        total_pages = []
        for pages in pages_by_type.values():
            total_pages.extend(list(pages))
        if len(total_pages) != len(set(total_pages)):
            raise Exception("Found pages classified in multiple ways")

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

        cache = FormatCache()

        # Generate tests
        for page_type, pages in pages_by_type.items():
            for page in pages:
                if page in pdf_extract_enabled:
                    yield PdfExtractTest.from_parent(
                        parent=self,
                        name=f"test_pdf_extract::[{page}]",
                        page_num=page,
                        page_type=page_type,
                        format_name=self.format_name,
                    )

                if page in text_filter_enabled:
                    yield TextFilterTest.from_parent(
                        parent=self,
                        name=f"test_text_filter[{page}]",
                        page_num=page,
                        page_type=page_type,
                        format_name=self.format_name,
                    )

                if page in deserialize_enabled:
                    yield DeserializeTest.from_parent(
                        parent=self,
                        name=f"test_deserialize[{page}]",
                        page_num=page,
                        page_type=page_type,
                        format_name=self.format_name,
                    )

        if pipeline_enabled:
            test = PipelineTest.from_parent(
                parent=self,
                name=f"test_pipeline",
                format_name=self.format_name,
                document_variant=self.document_variant,
            )
            test.add_marker(pytest.mark.integration_tests)
            yield test


class FreeportsFormat(Directory):
    def __init__(self, format_name, **kwargs):
        super().__init__(**kwargs)
        self.format_name = format_name
        self.cache = FormatCache()

    def get_document(self):
        return self.cache.doc.get_doc(self.path / "report.pdf", self.cache.pdf)

    def get_page(self, page_n):
        return self.cache.doc.get_page(self.path / "report.pdf", page_n, self.cache.pdf)

    def collect(self):
        directory = self.path
        multiple_documents = True
        documents = []
        for document in os.listdir(directory):
            if os.path.isdir(directory / document) and document not in ("pages", "out"):
                documents.append(document)
            else:
                if os.path.isfile(directory / document) and document == "report.pdf":
                    multiple_documents = False
        if multiple_documents:
            for document in documents:
                yield ReportVariant.from_parent(
                    parent=self,
                    name=f"report[{document}]" if document is not None else None,
                    path=self.path / document,
                    document_variant=document,
                )
        else:
            yield ReportVariant.from_parent(
                parent=self, name="report", path=self.path, document_variant=None
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
            path=path,
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
