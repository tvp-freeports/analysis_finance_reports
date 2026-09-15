"""Finding the page, which is the question ``inspect-page`` cannot be asked.

Every other development command starts from a page number. Getting one meant opening the document
outside the tooling: a merger note lives on one page of eleven hundred and nothing in
``freeports-dev`` could say which.

Two decisions are held here.

**The search may be a regular expression, and a selection may not.** ``PdfLineSelection`` matches
substrings with ``^`` and ``$`` anchors on purpose -- its algebra has to decide whether one
selection is a subset of, overlaps or is disjoint from another, and for regular expressions that is
undecidable. A search answers yes or no about one line and combines with nothing, so it is free to
be a regular expression, and it should be: the thing one gropes for is the thing one cannot yet
name exactly. The two must not be confused, which is why they are two options, ``--text`` and
``--regex``, rather than one that guesses.

**A hit is reported as it is found.** A directory of six hundred documents is minutes of reading;
a command that collects everything and prints at the end looks hung for all of them, and the first
hit is usually the one wanted. So the search is a generator and the caller prints as it goes --
which is also why there is no progress meter to write.
"""

from pathlib import Path

import pytest

from freeports_dev.find_text import (
    SearchError,
    documents_to_search,
    font_predicate,
    page_numbers,
    parse_pages,
    text_predicate,
)

from test_page_view import line  # noqa: E402  -- the same fake line, not a second one


class TestWhatCountsAsAMatch:
    """``--text`` is a substring, ``--regex`` is a regular expression, and neither is both."""

    def test_text_matches_a_substring_anywhere_in_the_line(self):
        assert text_predicate(text="net assets")("Total net assets as at")

    def test_text_ignores_case_by_default(self):
        assert text_predicate(text="NET ASSETS")("Total net assets")

    def test_text_can_be_made_case_sensitive(self):
        assert not text_predicate(text="NET ASSETS", ignore_case=False)(
            "Total net assets"
        )

    def test_text_is_not_a_regular_expression(self):
        """The trap this option exists to avoid: `.` is a full stop, not any character."""
        assert not text_predicate(text="net.assets")("Total net assets")
        assert text_predicate(text="net.assets")("Total net.assets")

    def test_a_regex_is_a_regular_expression(self):
        assert text_predicate(regex=r"Total\s+net\s+assets")("Total   net assets")

    def test_a_regex_ignores_case_by_default_too(self):
        assert text_predicate(regex=r"^total")("Total net assets")

    def test_a_malformed_regex_is_an_error_naming_it(self):
        with pytest.raises(SearchError, match=r"\[unclosed"):
            text_predicate(regex="[unclosed")

    def test_giving_neither_is_an_error_rather_than_matching_everything(self):
        with pytest.raises(SearchError, match="--text|--regex"):
            text_predicate()

    def test_giving_both_is_an_error_rather_than_a_silent_precedence(self):
        with pytest.raises(SearchError, match="both"):
            text_predicate(text="a", regex="b")


class TestFilteringByFont:
    """A corpus is often the only stable thing about a heading."""

    def test_no_font_asked_for_matches_every_font(self):
        assert font_predicate(None)(line("x", 0, 0, font="Whatever"))

    def test_a_font_matches_as_a_substring_and_ignores_case(self):
        assert font_predicate("frutiger")(line("x", 0, 0, font="Frutiger-Black"))

    def test_a_font_that_does_not_appear_does_not_match(self):
        assert not font_predicate("calibri")(line("x", 0, 0, font="Frutiger-Black"))


class TestThePageRange:
    """Narrowing the search when the region is roughly known."""

    def test_a_bare_number_is_that_page_alone(self):
        assert parse_pages("7") == (7, 7)

    def test_a_range_is_inclusive_at_both_ends(self):
        assert parse_pages("10-20") == (10, 20)

    def test_an_open_upper_end_runs_to_the_end_of_the_document(self):
        assert parse_pages("900-") == (900, None)

    def test_an_open_lower_end_starts_at_the_first_page(self):
        assert parse_pages("-40") == (1, 40)

    def test_nothing_asked_for_is_the_whole_document(self):
        assert parse_pages(None) == (1, None)

    def test_a_backwards_range_is_an_error_rather_than_an_empty_search(self):
        with pytest.raises(SearchError, match="20-10"):
            parse_pages("20-10")

    def test_nonsense_is_an_error_naming_what_was_given(self):
        with pytest.raises(SearchError, match="page range"):
            parse_pages("ten")

    def test_page_numbers_are_one_based_because_every_other_command_is(self):
        assert list(page_numbers((2, 4), total=10)) == [2, 3, 4]

    def test_a_range_past_the_end_stops_at_the_end(self):
        assert list(page_numbers((8, 100), total=10)) == [8, 9, 10]

    def test_a_range_wholly_past_the_end_yields_nothing(self):
        assert list(page_numbers((50, 60), total=10)) == []


class TestWhichDocumentsAreSearched:
    """One file, a directory of them, or the report a format's tests already pin."""

    def test_a_single_file_is_searched_alone(self, tmp_path):
        pdf = tmp_path / "report.pdf"
        pdf.write_bytes(b"%PDF-1.4\n")
        assert documents_to_search([pdf]) == [pdf]

    def test_a_directory_yields_its_pdfs_sorted(self, tmp_path):
        for name in ("b.pdf", "a.pdf", "notes.txt"):
            (tmp_path / name).write_bytes(b"x")
        assert [p.name for p in documents_to_search([tmp_path])] == ["a.pdf", "b.pdf"]

    def test_a_directory_is_searched_to_the_bottom(self, tmp_path):
        nested = tmp_path / "EURIZON-EN23" / "1"
        nested.mkdir(parents=True)
        (nested / "report.pdf").write_bytes(b"x")
        assert [p.name for p in documents_to_search([tmp_path])] == ["report.pdf"]

    def test_several_paths_are_searched_in_the_order_given(self, tmp_path):
        first, second = tmp_path / "z.pdf", tmp_path / "a.pdf"
        first.write_bytes(b"x")
        second.write_bytes(b"x")
        assert documents_to_search([first, second]) == [first, second]

    def test_the_same_document_named_twice_is_searched_once(self, tmp_path):
        pdf = tmp_path / "report.pdf"
        pdf.write_bytes(b"x")
        assert documents_to_search([pdf, pdf]) == [pdf]

    def test_a_path_that_is_not_there_is_an_error_naming_it(self, tmp_path):
        with pytest.raises(SearchError, match="missing.pdf"):
            documents_to_search([tmp_path / "missing.pdf"])

    def test_a_directory_holding_no_pdf_is_an_error_rather_than_a_silent_nothing(
        self, tmp_path
    ):
        with pytest.raises(SearchError, match="no PDF"):
            documents_to_search([tmp_path])

    def test_a_format_name_resolves_against_the_repository_test_corpus(self, tmp_path):
        base = tmp_path / "tests" / "formats" / "EURIZON-EN23"
        (base / "1").mkdir(parents=True)
        (base / "1" / "report.pdf").write_bytes(b"x")
        (base / "2").mkdir(parents=True)
        (base / "2" / "report.pdf").write_bytes(b"x")
        found = documents_to_search([], repo=tmp_path, format_name="EURIZON-EN23")
        assert [p.parent.name for p in found] == ["1", "2"]

    def test_a_format_with_one_report_resolves_to_that_report(self, tmp_path):
        base = tmp_path / "tests" / "formats" / "UBS-EN23"
        base.mkdir(parents=True)
        (base / "report.pdf").write_bytes(b"x")
        found = documents_to_search([], repo=tmp_path, format_name="UBS-EN23")
        assert found == [base / "report.pdf"]

    def test_an_unknown_format_is_an_error_naming_it(self, tmp_path):
        (tmp_path / "tests" / "formats").mkdir(parents=True)
        with pytest.raises(SearchError, match="NOSUCH-EN99"):
            documents_to_search([], repo=tmp_path, format_name="NOSUCH-EN99")

    def test_naming_neither_a_path_nor_a_format_is_an_error(self):
        with pytest.raises(SearchError, match="nothing to search"):
            documents_to_search([], repo=Path("."), format_name=None)
