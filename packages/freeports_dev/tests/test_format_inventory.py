"""Which tests a formats repository has — the one walk the suite and the coverage report share.

pytest discovers the tests in a formats repository by reading file names: a page with a
`<n>-pdf_blks.json` beside a `report.pdf` yields a pdf-extract test, a document with an `out/`
yields the whole-document test. The figures `tests.formats.integration` and
`tests.formats.single_page` are statements about exactly that walk.

**So there is one walk and both read it.** Two implementations of "which tests exist" would rot
apart, and the day they disagreed the report would say a document is covered while pytest collects
nothing for it — and nobody would find out, because the report is the only thing anybody reads.
These tests hold the rules; the plugin holding to them is checked by the suite of the real formats
repository, whose collection is unchanged by the factoring.

The two figures are deliberately not averaged into one. A repository can be strong in one and weak
in the other, and the formats repository in this workspace is exactly that: every format's stages
pinned page by page, and five documents nobody has ever run end to end. One number would hide it.
"""

import pytest

from freeports_dev.ci import formats as formats_coverage
from freeports_dev.format_inventory import (
    InventoryError,
    scan_variant,
    variant_paths,
)


def make_document(path, pages=(), out=False, report=True, page_type="investments"):
    """A document directory holding exactly the fixtures a test names.

    `pages` is a mapping of page number to the suffixes that page carries, so a test says what it
    means — `{3: ["pdf_blks", "txt_blks", "results"]}` is a page with the whole triple.
    """
    path.mkdir(parents=True, exist_ok=True)
    if report:
        (path / "report.pdf").write_bytes(b"%PDF-1.4\n")
    if out:
        (path / "out").mkdir()
    type_dir = path / "pages" / page_type
    type_dir.mkdir(parents=True, exist_ok=True)
    for number, kinds in dict(pages).items():
        for kind in kinds:
            (type_dir / f"{number}-{kind}.json").write_text("{}")
    return path


class TestWhatCountsAsADocument:
    """The unit is the document, which is why a repository of 26 formats holds 36 of them."""

    def test_a_format_holding_a_report_is_one_document(self, tmp_path):
        make_document(tmp_path / "AMUNDI-EN24", pages={1: ["pdf_blks"]})
        assert variant_paths(tmp_path / "AMUNDI-EN24") == [
            (None, tmp_path / "AMUNDI-EN24")
        ]

    def test_a_format_holding_subdirectories_is_one_document_each(self, tmp_path):
        root = tmp_path / "EURIZON-IT24"
        make_document(root / "1", pages={1: ["pdf_blks"]})
        make_document(root / "2", pages={1: ["pdf_blks"]})
        assert variant_paths(root) == [("1", root / "1"), ("2", root / "2")]

    def test_pages_and_out_are_not_documents_of_their_own(self, tmp_path):
        root = make_document(
            tmp_path / "AMUNDI-EN24", pages={1: ["pdf_blks"]}, out=True
        )
        assert variant_paths(root) == [(None, root)]

    def test_a_multi_document_format_is_listed_in_a_stable_order(self, tmp_path):
        root = tmp_path / "EURIZON-IT24"
        for name in ("3", "1", "2"):
            make_document(root / name, pages={1: ["pdf_blks"]})
        assert [variant for variant, _ in variant_paths(root)] == ["1", "2", "3"]

    def test_a_directory_that_is_not_there_holds_no_documents(self, tmp_path):
        assert variant_paths(tmp_path / "nothing") == []


class TestWhichTestsAPageYields:
    """A stage is testable when its input fixture and its output fixture are both present."""

    def test_pdf_extract_needs_the_blocks_fixture_and_a_document(self, tmp_path):
        document = make_document(tmp_path / "F", pages={1: ["pdf_blks"]})
        assert scan_variant(document, "F").pdf_extract_pages == {1}

    def test_and_yields_nothing_without_the_document(self, tmp_path):
        document = make_document(tmp_path / "F", pages={1: ["pdf_blks"]}, report=False)
        assert scan_variant(document, "F").pdf_extract_pages == set()

    def test_text_filter_needs_the_stage_before_it_and_its_own_output(self, tmp_path):
        document = make_document(tmp_path / "F", pages={1: ["pdf_blks", "txt_blks"]})
        inventory = scan_variant(document, "F")
        assert inventory.text_filter_pages == {1}
        assert inventory.deserialize_pages == set()

    def test_deserialize_needs_the_text_blocks_and_the_results(self, tmp_path):
        document = make_document(tmp_path / "F", pages={1: ["txt_blks", "results"]})
        assert scan_variant(document, "F").deserialize_pages == {1}

    def test_results_alone_pin_no_stage_at_all(self, tmp_path):
        document = make_document(tmp_path / "F", pages={1: ["results"]})
        inventory = scan_variant(document, "F")
        assert not inventory.pdf_extract_pages
        assert not inventory.text_filter_pages
        assert not inventory.deserialize_pages

    def test_filter_data_configures_the_test_and_is_not_a_fixture_of_it(self, tmp_path):
        document = make_document(tmp_path / "F", pages={1: ["filter_data"]})
        inventory = scan_variant(document, "F")
        assert inventory.pdf_blks == set()
        assert inventory.problems == []

    def test_pages_of_several_types_are_all_counted(self, tmp_path):
        document = make_document(tmp_path / "F", pages={1: ["pdf_blks"]})
        make_document(document, pages={7: ["pdf_blks"]}, page_type="holdings")
        assert scan_variant(document, "F").all_pages == {1, 7}


class TestTheIntegrationFigure:
    """A document counts when its whole-document test exists — which means it has an `out/`."""

    def test_a_document_with_an_out_directory_counts(self, tmp_path):
        document = make_document(tmp_path / "F", pages={1: ["pdf_blks"]}, out=True)
        assert scan_variant(document, "F").has_integration

    def test_one_without_does_not_however_many_pages_it_pins(self, tmp_path):
        document = make_document(
            tmp_path / "F", pages={1: ["pdf_blks", "txt_blks", "results"]}, out=False
        )
        assert not scan_variant(document, "F").has_integration

    def test_and_neither_does_one_with_no_document_to_run(self, tmp_path):
        document = make_document(
            tmp_path / "F", pages={1: ["pdf_blks"]}, out=True, report=False
        )
        assert not scan_variant(document, "F").has_integration


class TestTheSinglePageFigure:
    """A document counts when at least one page carries the whole triple."""

    def test_one_page_with_all_three_is_the_bar(self, tmp_path):
        document = make_document(
            tmp_path / "F", pages={4: ["pdf_blks", "txt_blks", "results"]}
        )
        inventory = scan_variant(document, "F")
        assert inventory.has_single_page_triple
        assert inventory.triple_pages == {4}

    def test_two_pages_each_holding_part_of_it_do_not_add_up(self, tmp_path):
        document = make_document(
            tmp_path / "F", pages={1: ["pdf_blks", "txt_blks"], 2: ["results"]}
        )
        assert not scan_variant(document, "F").has_single_page_triple

    def test_the_triple_may_be_spread_across_page_types(self, tmp_path):
        document = make_document(
            tmp_path / "F", pages={1: ["pdf_blks", "txt_blks", "results"]}
        )
        make_document(document, pages={9: ["pdf_blks"]}, page_type="holdings")
        assert scan_variant(document, "F").triple_pages == {1}

    def test_a_document_with_no_fixtures_at_all_does_not_count(self, tmp_path):
        document = make_document(tmp_path / "F", pages={}, out=True)
        assert not scan_variant(document, "F").has_single_page_triple


class TestADocumentTheRulesCannotDescribe:
    """Strict for pytest, which must fail at collection; forgiving for the report, which must print."""

    def test_a_missing_report_is_raised_when_strict(self, tmp_path):
        document = make_document(tmp_path / "F", pages={1: ["pdf_blks"]}, report=False)
        with pytest.raises(InventoryError) as raised:
            scan_variant(document, "F", strict=True)
        assert "report.pdf" in str(raised.value)

    def test_a_missing_pages_directory_is_raised_when_strict(self, tmp_path):
        (tmp_path / "F").mkdir()
        (tmp_path / "F" / "report.pdf").write_bytes(b"%PDF")
        with pytest.raises(InventoryError):
            scan_variant(tmp_path / "F", "F", strict=True)

    def test_a_stray_file_is_raised_when_strict(self, tmp_path):
        document = make_document(tmp_path / "F", pages={1: ["pdf_blks"]})
        (document / "pages" / "investments" / "1-nonsense.json").write_text("{}")
        with pytest.raises(InventoryError) as raised:
            scan_variant(document, "F", strict=True)
        assert "Unknown file" in str(raised.value)

    def test_the_same_stray_file_is_a_finding_when_not_strict(self, tmp_path):
        document = make_document(tmp_path / "F", pages={1: ["pdf_blks"]})
        (document / "pages" / "investments" / "1-nonsense.json").write_text("{}")
        inventory = scan_variant(document, "F", strict=False)
        assert any("Unknown file" in message for message in inventory.problems)

    def test_a_page_classified_two_ways_is_found(self, tmp_path):
        document = make_document(tmp_path / "F", pages={3: ["pdf_blks"]})
        make_document(document, pages={3: ["pdf_blks"]}, page_type="holdings")
        with pytest.raises(InventoryError) as raised:
            scan_variant(document, "F", strict=True)
        assert "multiple ways" in str(raised.value)

    def test_a_report_can_still_be_produced_for_a_broken_document(self, tmp_path):
        document = make_document(
            tmp_path / "F", pages={1: ["pdf_blks"]}, report=False, out=True
        )
        inventory = scan_variant(document, "F", strict=False)
        assert inventory.problems
        assert inventory.pdf_blks == {1}

    def test_a_file_with_no_page_number_is_ignored_rather_than_fatal(self, tmp_path):
        document = make_document(tmp_path / "F", pages={1: ["pdf_blks"]})
        (document / "pages" / "investments" / "notes-pdf_blks.json").write_text("{}")
        assert scan_variant(document, "F", strict=True).pdf_blks == {1}


class TestTheTwoRatios:
    def coverage_of(self, documents):
        return formats_coverage.FormatsCoverage(documents)

    def test_the_ratio_counts_documents_and_not_pages(self, tmp_path):
        one = make_document(
            tmp_path / "A", pages={1: ["pdf_blks", "txt_blks", "results"]}, out=True
        )
        two = make_document(
            tmp_path / "B", pages={n: ["pdf_blks"] for n in range(1, 20)}
        )
        coverage = self.coverage_of([scan_variant(one, "A"), scan_variant(two, "B")])
        assert coverage.ratio(coverage.with_integration) == 50.0
        assert coverage.ratio(coverage.with_single_page) == 50.0

    def test_the_documents_holding_a_figure_down_are_named_not_just_counted(
        self, tmp_path
    ):
        one = make_document(tmp_path / "A", pages={1: ["pdf_blks"]}, out=True)
        two = make_document(tmp_path / "B", pages={1: ["pdf_blks"]}, out=False)
        coverage = self.coverage_of([scan_variant(one, "A"), scan_variant(two, "B")])
        assert coverage.missing_integration == ["B"]

    def test_a_variant_is_named_the_way_a_person_writes_it(self, tmp_path):
        document = make_document(
            tmp_path / "EURIZON-IT24" / "3", pages={1: ["pdf_blks"]}
        )
        assert scan_variant(document, "EURIZON-IT24", "3").name == "EURIZON-IT24/3"

    def test_an_empty_repository_reads_a_hundred_rather_than_dividing_by_zero(self):
        """It has no untested document in it. Calling that 0 % refuses a blameless first commit."""
        coverage = self.coverage_of([])
        assert coverage.ratio(coverage.with_integration) == 100.0

    def test_the_measurements_carry_the_commit_they_were_taken_at(self, tmp_path):
        document = make_document(tmp_path / "A", pages={1: ["pdf_blks"]}, out=True)
        measured = self.coverage_of([scan_variant(document, "A")]).measurements(
            head="abc123"
        )
        assert {m.metric for m in measured} == {
            "tests.formats.integration",
            "tests.formats.single_page",
        }
        assert all(m.head == "abc123" and m.is_measured for m in measured)


class TestTheRenderings:
    def coverage_of(self, tmp_path):
        covered = make_document(
            tmp_path / "A", pages={1: ["pdf_blks", "txt_blks", "results"]}, out=True
        )
        bare = make_document(tmp_path / "B", pages={1: ["pdf_blks"]})
        return formats_coverage.FormatsCoverage(
            [scan_variant(covered, "A"), scan_variant(bare, "B")]
        )

    def test_the_text_rendering_names_what_is_missing(self, tmp_path):
        rendered = formats_coverage.render_text(self.coverage_of(tmp_path))
        assert "2 documents" in rendered
        assert "missing: B" in rendered

    def test_the_markdown_rendering_is_a_table(self, tmp_path):
        rendered = formats_coverage.render_markdown(self.coverage_of(tmp_path))
        assert rendered.startswith("| Metric |")
        assert "50.0 %" in rendered

    def test_the_badges_are_coloured_by_the_figure(self, tmp_path):
        rendered = formats_coverage.render_badges(self.coverage_of(tmp_path))
        assert "img.shields.io" in rendered
        assert "-red)" in rendered

    def test_the_json_rendering_is_keyed_by_metric(self, tmp_path):
        import json

        rendered = json.loads(formats_coverage.render_json(self.coverage_of(tmp_path)))
        assert set(rendered) == {
            "tests.formats.integration",
            "tests.formats.single_page",
        }
        assert rendered["tests.formats.integration"]["detail"]["missing"] == ["B"]

    def test_a_singular_document_is_not_called_documents(self, tmp_path):
        one = make_document(tmp_path / "A", pages={1: ["pdf_blks"]}, out=True)
        coverage = formats_coverage.FormatsCoverage([scan_variant(one, "A")])
        assert "1 document in" in formats_coverage.render_text(coverage)
