import pytest
import os
from pathlib import Path
from pytest import Collector, Item
from freeports_analysis.formats.data import VALID_FORMATS
from freeports_analysis.formats.algorithms import Algorithm
from freeports_analysis.data import get_target_companies
import freeports_lib

test_companies_df = get_target_companies(["TEST"])
test_companies = test_companies = (
    freeports_lib.text_filter.matcher.CompanyMatchInfos.compile_from_pandas_df(
        test_companies_df
    )
)

# class PdfExtractSinglePage(Item):


# class FreeportsDocumentDir():


# def pytest_collect_file(parent, file_path):
#     if file_path.suffix == ".yaml" and file_path.name.startswith("test"):
#         return YamlFile.from_parent(parent, path=file_path)


# class YamlFile(pytest.File):
#     def collect(self):
#         # We need a yaml parser, e.g. PyYAML.
#         import yaml

#         raw = yaml.safe_load(self.path.open(encoding="utf-8"))
#         for name, spec in sorted(raw.items()):
#             yield YamlItem.from_parent(self, name=name, spec=spec)


class FreeportsFormat(Collector):
    def __init__(self, name, parent, format_name, **kwargs):
        super().__init__(name=name, parent=parent, **kwargs)
        self.format_name = format_name

    def collect(self):
        directory = self.path / self.format_name
        pdf_blks = set()
        txt_blks = set()
        results = set()
        pages = {}
        tot_pages = None
        classified_pages = set()
        is_report_present = False
        is_out_present = False
        pdf_extract_enabled = set()
        text_filter_enabled = set()
        deserialize_enabled = set()
        page_classification_enabled = set()
        pipeline_enabled = False
        for d in os.listdir(directory):
            if d == "pages":
                for pt in os.listdir(directory / d):
                    for f in os.listdir(directory / "pages" / pt):
                        page_n, type_pkl = f.split("-")
                        page_n = int(page_n)
                        if pt in pages:
                            pages[pt].add(page_n)
                        else:
                            pages[pt] = set()
                        classified_pages.add((pt, page_n))
                        if type_pkl == "pdf_blks.pkl":
                            pdf_blks.add(page_n)
                        elif type_pkl == "txt_blks.pkl":
                            txt_blks.add(page_n)
                        elif type_pkl == "results.pkl":
                            results.add(page_n)
                        else:
                            raise Exception(
                                f"File not known in pages folder in {directory}"
                            )
                pgs = [p for pset in pages.values() for p in pset]
                if pgs != list(set(pgs)):
                    raise Exception("Found two pages classified in different way")
                tot_pages = set(pgs)
            elif d == "out":
                is_out_present = True
            elif d == "report.pdf":
                is_report_present = True
            else:
                raise Exception(f"File {d} not known in {directory}")

        for p in tot_pages:
            if is_report_present and p in pdf_blks:
                pdf_extract_enabled.add(p)
                page_classification_enabled.add(p)
            if p in pdf_blks and p in txt_blks:
                text_filter_enabled.add(p)
            if p in txt_blks and p in results:
                deserialize_enabled.add(p)
        if is_report_present and is_out_present:
            pipeline_enabled = True

        print("pdf_blks:", pdf_extract_enabled)
        print("txt_blks:", text_filter_enabled)
        print("results:", deserialize_enabled)
        print("pipeline_enabled", pipeline_enabled)

        return []
        # ihook = self.ihook
        # for d in os.listdir(self.path):
        #     yield from ihook.pytest_collect_directory(
        #         path=self.path / d,parent=self
        #     )


@pytest.hookimpl
def pytest_collect_directory(path, parent):
    dirname = path.stem
    if dirname in VALID_FORMATS:
        return FreeportsFormat.from_parent(
            parent=parent, name=f"FreeportsFormat{{{dirname}}}", format_name=dirname
        )
    print("collect dir", path)


def pytest_collect_file(file_path, parent):
    print("collect file", file_path, parent)
