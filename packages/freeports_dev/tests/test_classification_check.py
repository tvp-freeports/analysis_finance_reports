"""The rule of the document-classification test, and the reading of `freeports classify`.

Phase 13 of `agent-memory/PLAN-document-classification.md`: every test report of a formats
repository gets one test asking whether the engine recognises it as the format of its folder. The
rule is asymmetric on purpose. **Not classified passes** — a repository whose rules are still being
written would otherwise fail on every document it has not reached yet — and so does a partial the
folder's format refines (SPEC §1.5): the engine stopped early, but on the right road. **Classified
as another format fails**, and so does a partial that excludes the folder: that is the one outcome
which would send a real run down the wrong algorithm.

The engine is asked through `freeports classify`, whose default output is one line per document
(`DESIGN-phase10-classify.md` §5):

    report: MEDIOLANUM-IT24.A
    report: MEDIOLANUM-IT24.A (by exclusion)
    report: partial MEDIOLANUM-IT24.* — candidates: MEDIOLANUM-IT24.A, MEDIOLANUM-IT24.B
    report: not classified
    report: error: <message>

The candidates of a partial are exactly the formats of the repository that refine it, so "the
folder refines the partial" is "the folder is among the candidates": nothing here re-implements
the engine's pattern relation.

Everything in this file is pure and fast; the tests that start `freeports classify` or a pytest
session are in `test_classification_test_collected.py`, marked slow.
"""

import pytest

from freeports_dev.classification_check import (
    ClassifyOutcome,
    ClassifyOutputError,
    classification_failure,
    parse_classify_output,
)


class TestReadingACompleteOutcome:
    def test_a_complete_format_is_read_as_complete(self):
        outcome = parse_classify_output("report: MEDIOLANUM-IT24.A\n")
        assert outcome == ClassifyOutcome.complete("MEDIOLANUM-IT24.A")

    def test_a_format_decided_by_exclusion_is_complete_and_says_so(self):
        outcome = parse_classify_output("report: MEDIOLANUM-IT24.A (by exclusion)\n")
        assert outcome == ClassifyOutcome.complete(
            "MEDIOLANUM-IT24.A", by_exclusion=True
        )

    def test_a_format_with_a_country_keeps_it(self):
        outcome = parse_classify_output("report: FINECO-EN23@IR\n")
        assert outcome == ClassifyOutcome.complete("FINECO-EN23@IR")

    def test_a_missing_final_newline_does_not_matter(self):
        assert parse_classify_output("report: AMUNDI-EN24") == ClassifyOutcome.complete(
            "AMUNDI-EN24"
        )


class TestReadingAPartialOutcome:
    def test_a_partial_carries_its_pattern_and_candidates_in_order(self):
        outcome = parse_classify_output(
            "report: partial MEDIOLANUM-IT24.* — candidates: "
            "MEDIOLANUM-IT24.A, MEDIOLANUM-IT24.B\n"
        )
        assert outcome == ClassifyOutcome.partial(
            "MEDIOLANUM-IT24.*", ["MEDIOLANUM-IT24.A", "MEDIOLANUM-IT24.B"]
        )

    def test_a_partial_with_a_single_candidate_stays_partial(self):
        """Plan §2: no automatic completion of a partial with one candidate."""
        outcome = parse_classify_output(
            "report: partial EURIZON — candidates: EURIZON-EN21\n"
        )
        assert outcome == ClassifyOutcome.partial("EURIZON", ["EURIZON-EN21"])


class TestReadingTheOtherOutcomes:
    def test_not_classified(self):
        assert (
            parse_classify_output("report: not classified\n")
            == ClassifyOutcome.not_classified()
        )

    def test_an_error_keeps_its_whole_message(self):
        outcome = parse_classify_output(
            "report: error: page 1 could not be read: boom\n"
        )
        assert outcome == ClassifyOutcome.error("page 1 could not be read: boom")


class TestFindingTheDocumentsLine:
    def test_only_the_line_of_the_label_asked_for_is_read(self):
        stdout = "other: AMUNDI-EN24\nreport: not classified\n"
        assert parse_classify_output(stdout) == ClassifyOutcome.not_classified()

    def test_the_label_can_be_chosen(self):
        stdout = "a.pdf: AMUNDI-EN24\nb.pdf: not classified\n"
        assert parse_classify_output(stdout, label="a.pdf") == ClassifyOutcome.complete(
            "AMUNDI-EN24"
        )

    def test_a_label_that_only_starts_like_ours_is_not_ours(self):
        with pytest.raises(ClassifyOutputError):
            parse_classify_output("report.pdf: AMUNDI-EN24\n")

    def test_no_line_for_the_document_is_an_error_not_a_pass(self):
        """An empty stdout must never be read as "not classified": that would pass silently."""
        with pytest.raises(ClassifyOutputError):
            parse_classify_output("")

    def test_a_line_in_no_known_form_is_an_error(self):
        with pytest.raises(ClassifyOutputError):
            parse_classify_output("report: partial\n")


class TestTheRule:
    """`classification_failure` is `None` for a pass, the failure message otherwise."""

    def test_classified_as_the_folder_passes(self):
        assert (
            classification_failure(
                "AMUNDI-EN24", ClassifyOutcome.complete("AMUNDI-EN24")
            )
            is None
        )

    def test_classified_as_the_folder_by_exclusion_passes(self):
        outcome = ClassifyOutcome.complete("ASTERIA-EN23", by_exclusion=True)
        assert classification_failure("ASTERIA-EN23", outcome) is None

    def test_not_classified_passes(self):
        assert (
            classification_failure("AMUNDI-EN24", ClassifyOutcome.not_classified())
            is None
        )

    def test_classified_as_another_format_fails(self):
        assert (
            classification_failure(
                "AMUNDI-IT24", ClassifyOutcome.complete("AMUNDI-EN24")
            )
            is not None
        )

    def test_the_failure_names_the_folder_and_the_format_found(self):
        message = classification_failure(
            "AMUNDI-IT24", ClassifyOutcome.complete("AMUNDI-EN24")
        )
        assert "AMUNDI-IT24" in message
        assert "AMUNDI-EN24" in message

    def test_a_close_relative_is_still_another_format(self):
        """Same entity, same year, other variant: a different algorithm, so a failure."""
        outcome = ClassifyOutcome.complete("MEDIOLANUM-IT24.B")
        assert classification_failure("MEDIOLANUM-IT24.A", outcome) is not None

    def test_a_partial_the_folder_refines_passes(self):
        outcome = ClassifyOutcome.partial(
            "MEDIOLANUM-IT24.*", ["MEDIOLANUM-IT24.A", "MEDIOLANUM-IT24.B"]
        )
        assert classification_failure("MEDIOLANUM-IT24.B", outcome) is None

    def test_an_entity_only_partial_the_folder_refines_passes(self):
        outcome = ClassifyOutcome.partial(
            "MEDIOLANUM", ["MEDIOLANUM-EN23", "MEDIOLANUM-IT24.A"]
        )
        assert classification_failure("MEDIOLANUM-EN23", outcome) is None

    def test_a_partial_the_folder_does_not_refine_fails(self):
        outcome = ClassifyOutcome.partial(
            "MEDIOLANUM-IT24.*", ["MEDIOLANUM-IT24.A", "MEDIOLANUM-IT24.B"]
        )
        assert classification_failure("MEDIOLANUM-EN23", outcome) is not None

    def test_the_failure_of_a_partial_names_the_partial_and_the_folder(self):
        outcome = ClassifyOutcome.partial("EURIZON", ["EURIZON-EN21", "EURIZON-EN23"])
        message = classification_failure("AMUNDI-EN24", outcome)
        assert "EURIZON" in message
        assert "AMUNDI-EN24" in message

    def test_a_partial_whose_candidate_merely_contains_the_folder_name_fails(self):
        """Membership is by whole name, not by substring."""
        outcome = ClassifyOutcome.partial("X-IT23@LUX.*", ["X-IT23@LUX.AB"])
        assert classification_failure("X-IT23@LUX.A", outcome) is not None

    def test_an_error_fails_and_carries_the_engines_message(self):
        """A classifier that errs has not said "not classified": it has malfunctioned."""
        message = classification_failure(
            "AMUNDI-EN24", ClassifyOutcome.error("page 1 could not be read: boom")
        )
        assert message is not None
        assert "page 1 could not be read: boom" in message
