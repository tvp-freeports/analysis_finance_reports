import json as _json
import os
from pathlib import Path

import pandas as pd
import pytest
import yaml
from abc import ABC, abstractclassmethod
from pymupdf import Document
from pytest import Collector, Function, Directory

from freeports.core import Algorithm
from freeports.cli import run_job
from freeports.formats_repo import get_formats

from freeports_dev.serialization import load as json_load
from freeports_dev.serialization import from_serializable
import _pytest.fixtures as fixtures

_formats_cache = {"valid": None, "repo_dir": None}


def _get_valid_formats(session):
    rootdir = Path(session.config.rootdir)
    formats_csv = rootdir / "metadata" / "formats.csv"
    if formats_csv.exists():
        if _formats_cache["valid"] is None:
            _formats_cache["valid"] = set(get_formats(rootdir))
            _formats_cache["repo_dir"] = rootdir
        return _formats_cache["valid"]
    return set()


def _get_test_companies(rootdir=None):
    from freeports_dev.input_db import get_test_companies as gtc

    return gtc(rootdir)


def _target_lists():
    """The lists both the fixture loader and the pipeline test search for.

    Read through the configuration rather than written `["TEST"]` in two places, so that a repository
    whose tests are built against a different list says so once. It still resolves to `TEST` when
    nothing says otherwise, which is what every format repository in existence relies on.
    """
    from freeports_dev.config import active

    return active().target_lists


def _load_filter_data(cache, variant_path, page_type):
    """Load filter_data from filter_data.json or fall back to test companies.

    filter_data.json can contain:
      - {"target_lists": ["TEST", "CUSTOM"]}  to specify which input DB lists to use
      - A list of serialized objects (intermediate results for multi-iteration formats)

    If no filter_data.json exists, defaults to the test input DB's companies.
    """
    filter_path = variant_path / "pages" / page_type / "filter_data.json"
    if filter_path.exists():
        data = json_load(open(filter_path, "r", encoding="utf-8"))
        if isinstance(data, dict) and "target_lists" in data:
            from freeports_dev.input_db import get_test_companies as gtc

            return gtc(_formats_cache["repo_dir"], data["target_lists"])
        return data
    return _get_test_companies(_formats_cache["repo_dir"])


class AlgorithmCache:
    def __init__(self):
        self.algorithms = {}

    def load(self, format_name):
        if format_name not in self.algorithms:
            self.algorithms[format_name] = Algorithm.load(
                _formats_cache["repo_dir"],
                format_name,
                list(_formats_cache["valid"]),
            )
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
        df = pd.read_csv(path, index_col=False, encoding="utf-8")
        for col in df.columns:
            if isinstance(df[col].dtype, pd.StringDtype):
                df[col] = df[col].astype(object)
        return df


class YamlCache(FileCache):
    def load_content(self, path):
        return yaml.safe_load(path.open("r"))


class JsonCache(FileCache):
    def load_content(self, path):
        return json_load(open(path, "r", encoding="utf-8"))


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
        self.json = JsonCache()
        self.pdf = PdfCache()
        self.doc = DocumentCache()


def _load_test_fixture(cache, base_dir, page_type, page_num, file_stem):
    """Load a test fixture from a JSON file."""
    json_path = base_dir / "pages" / page_type / f"{page_num}-{file_stem}.json"
    if not json_path.exists():
        raise FileNotFoundError(f"Test fixture not found: {json_path}")
    return cache.json.get_file(json_path)


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

        expected = _load_test_fixture(
            self.parent.cache,
            self.parent.path,
            self.page_type,
            self.page_num,
            "pdf_blks",
        )
        assert frozenset(result) == frozenset(expected), (
            f"PDF extract failed for page {self.page_num} ({self.page_type})"
        )


class TextFilterTest(Function):
    def __init__(self, name, parent, page_num, page_type, format_name, **kwargs):
        super().__init__(name=name, parent=parent, callobj=self.runtest, **kwargs)
        self.page_num = page_num
        self.page_type = page_type
        self.format_name = format_name

    def runtest(self):
        algorithm = self.parent.cache.algorithm.load(self.format_name)
        filter_data = _load_filter_data(
            self.parent.cache, self.parent.path, self.page_type
        )

        page_content = self.parent.get_page(self.page_num)
        result = algorithm.apply_text_filter(page_content, filter_data, self.page_type)

        expected = _load_test_fixture(
            self.parent.cache,
            self.parent.path,
            self.page_type,
            self.page_num,
            "txt_blks",
        )
        assert frozenset(result) == frozenset(expected), (
            f"Text filter failed for page {self.page_num} ({self.page_type})"
        )


class DeserializeTest(Function):
    def __init__(self, name, parent, page_num, page_type, format_name, **kwargs):
        super().__init__(name=name, parent=parent, callobj=self.runtest, **kwargs)
        self.page_num = page_num
        self.page_type = page_type
        self.format_name = format_name

    def runtest(self):
        algorithm = self.parent.cache.algorithm.load(self.format_name)
        filter_data = _load_filter_data(
            self.parent.cache, self.parent.path, self.page_type
        )

        page_content = self.parent.get_page(self.page_num)
        result = algorithm.apply_deserialize(page_content, filter_data, self.page_type)

        expected = _load_test_fixture(
            self.parent.cache,
            self.parent.path,
            self.page_type,
            self.page_num,
            "results",
        )
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
        create_dir = self._request.getfixturevalue("tmp_path_factory")
        tmp_dir = f"{self.format_name}" + (
            "" if self.document_variant is None else f"__report_{self.document_variant}"
        )
        from freeports_dev.input_db import resolve_input_db

        input_db = resolve_input_db(_formats_cache["repo_dir"])
        out_path = create_dir.mktemp(tmp_dir, numbered=False)
        run_job(
            input_reports=[(None, str(self.parent.path / "report.pdf"), "report")],
            format=self.format_name,
            target_lists=_target_lists(),
            formats_repo_path=str(_formats_cache["repo_dir"]),
            input_db_path=str(input_db),
            out_path=str(out_path),
            save_pdf=False,
        )

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
        pdf_blks = set()
        txt_blks = set()
        results = set()
        pages_by_type = {}
        all_pages = set()

        has_report = (directory / "report.pdf").exists()
        has_out = (directory / "out").exists()

        if not has_report:
            raise self.CollectError(f"Missing report.pdf in {directory}")

        pages_dir = directory / "pages"
        if not pages_dir.exists():
            raise self.CollectError(f"Missing pages directory in {directory}")

        for page_type in os.listdir(pages_dir):
            type_dir = pages_dir / page_type
            if not type_dir.is_dir():
                continue

            pages_by_type[page_type] = set()

            for f in os.listdir(type_dir):
                if "-" not in f:
                    continue

                parts = f.split("-", 1)
                if len(parts) != 2:
                    continue
                page_num_str, file_type = parts
                try:
                    page_num = int(page_num_str)
                except ValueError:
                    continue

                pages_by_type[page_type].add(page_num)
                all_pages.add(page_num)

                if file_type == "pdf_blks.json":
                    pdf_blks.add(page_num)
                elif file_type == "txt_blks.json":
                    txt_blks.add(page_num)
                elif file_type == "results.json":
                    results.add(page_num)
                elif file_type in ("filter_data.json"):
                    pass
                else:
                    raise Exception(f"Unknown file in pages folder: {f}")

        total_pages = []
        for pages in pages_by_type.values():
            total_pages.extend(list(pages))
        if len(total_pages) != len(set(total_pages)):
            raise Exception("Found pages classified in multiple ways")

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
                name="test_pipeline",
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
            if os.path.isdir(directory / document) and document not in (
                "pages",
                "out",
            ):
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
                parent=self,
                name="report",
                path=self.path,
                document_variant=None,
            )


@pytest.hookimpl(tryfirst=True)
def pytest_collect_directory(path, parent):
    path = Path(path)
    dirname = path.name
    valid_formats = _get_valid_formats(parent.session)

    if dirname in valid_formats:
        return FreeportsFormat.from_parent(
            parent=parent,
            name=f"FreeportsFormat[{dirname}]",
            format_name=dirname,
            path=path,
        )
    return None


@pytest.hookimpl(tryfirst=True)
def pytest_collect_file(file_path, parent):
    file_path = Path(file_path)
    valid_formats = _get_valid_formats(parent.session)

    for part in file_path.parts:
        if part in valid_formats:
            return None
    return None
