"""The document-classification test, as a formats repository's suite collects and runs it.

Phase 13 of `agent-memory/PLAN-document-classification.md`. Each case builds a small formats
repository — a `formats.csv`, a `document_classify.yaml` with one text criterion per step, and a
generated one-page PDF per document — and runs pytest on it in a separate process, so that the
plugin collects it exactly as it collects `analysis_finance_reports_formats`.

The pure rule is pinned in `test_classification_check.py`; these are the end-to-end cases the plan
asks for: a folder that classifies right, one that does not classify, one that classifies wrong
(expected to fail), and the two partial cases. They run the real `freeports classify`, so they
need the engine built with phase 10 on the PATH (the venv's `freeports`).

The error branches — a classifier that does not load, a document whose classification errs — are
run against a fake `freeports` placed first on the PATH, so that they do not depend on which
engine message a given mistake produces.

**Slow.** Every case starts at least two processes (pytest, then `freeports classify`), so each asks
for `runs_a_process`, which `conftest.py` turns into the `slow` marker.
"""

import json
import os
import stat
import sys
import textwrap

import pytest

from freeports_dev import cli

pytest_plugins = ["pytester"]


@pytest.fixture
def runs_a_process():
    """Marks a test that starts a real process: see ``conftest.SLOW_FIXTURES``."""


FORMATS = ["ACME-EN24", "ACME-IT24", "BETA-EN24"]

#: Leaves first, then the partial root: a found partial is sticky (SPEC §9.2).
RULES = """\
- 1:
    ACME-EN24: {text: '^ACME FUNDS'}
    BETA-EN24: {text: '^BETA FUNDS'}
    ACME: {text: '^ACME'}
"""

#: The first line of each kind of generated document, by what the rules above make of it.
CLASSIFIED_ACME_EN24 = "ACME FUNDS"
CLASSIFIED_BETA_EN24 = "BETA FUNDS"
PARTIAL_ACME = "ACME GROUP"
UNCLASSIFIED = "Annual report"

CONFTEST = """\
def pytest_configure(config):
    config.addinivalue_line(
        "markers",
        "integration_tests: mark test as an test that test the whole pipeline and that is slower",
    )
"""


def _formats_csv_row(name):
    """`ACME-EN24` → `ACME,EN,24,,` — the three columns `get_formats` composes a name from."""
    entity, rest = name.split("-", 1)
    return f"{entity},{rest[:2]},{rest[2:]},,"


def _write_pdf(path, first_line):
    import pymupdf

    document = pymupdf.open()
    page = document.new_page()
    page.insert_text((72, 72), first_line, fontsize=20)
    page.insert_text((72, 120), "Unaudited semi-annual statements", fontsize=10)
    path.parent.mkdir(parents=True, exist_ok=True)
    document.save(str(path))


def make_repo(root, documents, rules=RULES, formats=FORMATS):
    """A formats repository holding `documents`: `{"ACME-EN24": text}` or `{"ACME-EN24/2": text}`.

    Each document gets a `report.pdf` whose first line is `text` and an empty `pages/` — enough
    for the plugin to collect it, and nothing for any test but the classification one.
    `rules=None` leaves `document_classify.yaml` out altogether.
    """
    (root / "metadata").mkdir(parents=True, exist_ok=True)
    (root / "metadata" / "formats.csv").write_text(
        "Name,Locale,Year,Country,Version\n"
        + "".join(_formats_csv_row(name) + "\n" for name in formats)
    )
    (root / "metadata" / "url_mapping.csv").write_text("Format name,Url\n")
    if rules is not None:
        (root / "metadata" / "document_classify.yaml").write_text(rules)
    (root / "pytest.ini").write_text("[pytest]\n")
    (root / "tests").mkdir(exist_ok=True)
    (root / "tests" / "conftest.py").write_text(CONFTEST)
    for name, first_line in documents.items():
        directory = root / "tests" / "formats" / name
        _write_pdf(directory / "report.pdf", first_line)
        (directory / "pages" / "investments").mkdir(parents=True, exist_ok=True)
    return root


def run_suite(pytester, *arguments):
    return pytester.runpytest_subprocess(
        "-v", "-rA", "-p", "no:cacheprovider", *arguments
    )


# ------------------------------------------------------------------------------------------------
# The five outcomes the plan names, against the real engine
# ------------------------------------------------------------------------------------------------


class TestAgainstTheRealEngine:
    def test_a_folder_classified_as_itself_passes(self, pytester, runs_a_process):
        make_repo(pytester.path, {"ACME-EN24": CLASSIFIED_ACME_EN24})
        result = run_suite(pytester)
        result.stdout.fnmatch_lines(["*test_classification*PASSED*"])
        result.assert_outcomes(passed=1)

    def test_a_folder_not_classified_passes(self, pytester, runs_a_process):
        make_repo(pytester.path, {"BETA-EN24": UNCLASSIFIED})
        result = run_suite(pytester)
        result.stdout.fnmatch_lines(["*test_classification*PASSED*"])
        result.assert_outcomes(passed=1)

    def test_a_folder_classified_as_another_format_fails(
        self, pytester, runs_a_process
    ):
        make_repo(pytester.path, {"BETA-EN24": CLASSIFIED_ACME_EN24})
        result = run_suite(pytester)
        result.stdout.fnmatch_lines(["*test_classification*FAILED*"])
        result.assert_outcomes(failed=1)

    def test_the_failure_names_both_formats(self, pytester, runs_a_process):
        make_repo(pytester.path, {"BETA-EN24": CLASSIFIED_ACME_EN24})
        output = run_suite(pytester).stdout.str()
        assert "ACME-EN24" in output
        assert "BETA-EN24" in output

    def test_a_partial_the_folder_refines_passes(self, pytester, runs_a_process):
        make_repo(pytester.path, {"ACME-IT24": PARTIAL_ACME})
        result = run_suite(pytester)
        result.stdout.fnmatch_lines(["*test_classification*PASSED*"])
        result.assert_outcomes(passed=1)

    def test_a_partial_the_folder_does_not_refine_fails(self, pytester, runs_a_process):
        make_repo(pytester.path, {"BETA-EN24": PARTIAL_ACME})
        result = run_suite(pytester)
        result.stdout.fnmatch_lines(["*test_classification*FAILED*"])
        result.assert_outcomes(failed=1)

    def test_a_repository_with_no_rules_passes_every_document(
        self, pytester, runs_a_process
    ):
        """Before phase 14 the real formats repository has no `document_classify.yaml` at all:
        every document is "not classified", and that is a pass, not a broken suite."""
        make_repo(
            pytester.path,
            {"ACME-EN24": CLASSIFIED_ACME_EN24, "BETA-EN24": CLASSIFIED_ACME_EN24},
            rules=None,
        )
        run_suite(pytester).assert_outcomes(passed=2)


class TestOneTestPerDocument:
    def test_each_document_of_a_multi_document_format_has_its_own(
        self, pytester, runs_a_process
    ):
        make_repo(
            pytester.path,
            {"BETA-EN24/1": CLASSIFIED_BETA_EN24, "BETA-EN24/2": CLASSIFIED_ACME_EN24},
        )
        result = run_suite(pytester)
        result.assert_outcomes(passed=1, failed=1)
        result.stdout.fnmatch_lines(["*report?2?*test_classification*FAILED*"])

    def test_every_format_folder_gets_one(self, pytester, runs_a_process):
        make_repo(
            pytester.path,
            {
                "ACME-EN24": CLASSIFIED_ACME_EN24,
                "ACME-IT24": UNCLASSIFIED,
                "BETA-EN24": CLASSIFIED_BETA_EN24,
            },
        )
        result = run_suite(pytester, "--collect-only", "-q")
        collected = [
            line for line in result.stdout.lines if "test_classification" in line
        ]
        assert len(collected) == 3


# ------------------------------------------------------------------------------------------------
# Out of the commit gate
# ------------------------------------------------------------------------------------------------


class TestItStaysOutOfTheCommitGate:
    """The commit gate of a formats repository runs `freeports-dev test --fast` (`make test-fast`,
    `ci-fast`), whose selection is `cli.TEST_SELECTIONS["fast"]`. Opening a PDF is slow, so the
    classification test must not be in it — and must be in `--slow`, or it would never run.

    Asked through the selections themselves rather than a marker name, so that this holds whichever
    marker carries it."""

    def collected(self, pytester, selection):
        marker = cli.TEST_SELECTIONS[selection][0]
        result = run_suite(pytester, "--collect-only", "-q", "-m", marker)
        return [line for line in result.stdout.lines if "test_classification" in line]

    def test_the_fast_selection_leaves_it_out(self, pytester, runs_a_process):
        make_repo(pytester.path, {"ACME-EN24": CLASSIFIED_ACME_EN24})
        assert self.collected(pytester, "fast") == []

    def test_the_slow_selection_takes_it(self, pytester, runs_a_process):
        make_repo(pytester.path, {"ACME-EN24": CLASSIFIED_ACME_EN24})
        assert len(self.collected(pytester, "slow")) == 1

    def test_it_carries_a_registered_marker(self, pytester, runs_a_process):
        """`--strict-markers` must not turn the repository's suite into a collection error."""
        make_repo(pytester.path, {"ACME-EN24": CLASSIFIED_ACME_EN24})
        result = run_suite(pytester, "--strict-markers", "--collect-only", "-q")
        assert result.ret == 0
        assert any("test_classification" in line for line in result.stdout.lines)


# ------------------------------------------------------------------------------------------------
# How the engine is asked, and what happens when it cannot answer — against a fake `freeports`
# ------------------------------------------------------------------------------------------------


@pytest.fixture
def fake_freeports(tmp_path, monkeypatch, runs_a_process):
    """A `freeports` first on the PATH that prints what the test tells it to and records its argv.

    `configure(stdout=..., stderr=..., exit=...)`; `calls()` is the list of argv it received.
    """
    bin_dir = tmp_path / "fake-bin"
    bin_dir.mkdir()
    script = bin_dir / "freeports"
    behaviour = tmp_path / "fake-behaviour.json"
    log = tmp_path / "fake-calls.jsonl"
    script.write_text(
        textwrap.dedent(
            f"""\
            #!{sys.executable}
            import json, sys
            with open({str(log)!r}, "a") as log:
                log.write(json.dumps(sys.argv[1:]) + "\\n")
            behaviour = json.load(open({str(behaviour)!r}))
            sys.stdout.write(behaviour["stdout"])
            sys.stderr.write(behaviour["stderr"])
            sys.exit(behaviour["exit"])
            """
        )
    )
    script.chmod(script.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    monkeypatch.setenv("PATH", f"{bin_dir}{os.pathsep}{os.environ['PATH']}")

    class Fake:
        def configure(self, stdout="", stderr="", exit=0):
            behaviour.write_text(
                json.dumps({"stdout": stdout, "stderr": stderr, "exit": exit})
            )

        def calls(self):
            if not log.exists():
                return []
            return [json.loads(line) for line in log.read_text().splitlines()]

    fake = Fake()
    fake.configure(stdout="report: not classified\n")
    return fake


class TestClassifyDocument:
    """`classify_document` runs `freeports classify` once per document and reads its line."""

    def test_it_runs_the_classify_subcommand_on_the_document_against_the_repo(
        self, tmp_path, fake_freeports
    ):
        from freeports_dev.classification_check import classify_document

        repo = make_repo(tmp_path / "repo", {"ACME-EN24": CLASSIFIED_ACME_EN24})
        pdf = repo / "tests" / "formats" / "ACME-EN24" / "report.pdf"
        classify_document(pdf, repo)
        (call,) = fake_freeports.calls()
        assert call[0] == "classify"
        assert any(str(pdf) in argument for argument in call)
        assert any(str(repo) in argument for argument in call)

    def test_it_returns_the_outcome_printed(self, tmp_path, fake_freeports):
        from freeports_dev.classification_check import (
            ClassifyOutcome,
            classify_document,
        )

        repo = make_repo(tmp_path / "repo", {"ACME-EN24": CLASSIFIED_ACME_EN24})
        fake_freeports.configure(stdout="report: ACME-EN24\n")
        outcome = classify_document(
            repo / "tests" / "formats" / "ACME-EN24" / "report.pdf", repo
        )
        assert outcome == ClassifyOutcome.complete("ACME-EN24")

    def test_a_document_that_could_not_be_read_is_an_error_outcome(
        self, tmp_path, fake_freeports
    ):
        """Exit 3 still prints the document's line (`error: …`): it is an outcome, not a crash."""
        from freeports_dev.classification_check import (
            ClassifyOutcome,
            classify_document,
        )

        repo = make_repo(tmp_path / "repo", {"ACME-EN24": CLASSIFIED_ACME_EN24})
        fake_freeports.configure(
            stdout="report: error: page 1 could not be read: boom\n", exit=3
        )
        outcome = classify_document(
            repo / "tests" / "formats" / "ACME-EN24" / "report.pdf", repo
        )
        assert outcome == ClassifyOutcome.error("page 1 could not be read: boom")

    def test_a_command_failing_without_an_outcome_raises_with_its_stderr(
        self, tmp_path, fake_freeports
    ):
        from freeports_dev.classification_check import (
            ClassifyOutputError,
            classify_document,
        )

        repo = make_repo(tmp_path / "repo", {"ACME-EN24": CLASSIFIED_ACME_EN24})
        fake_freeports.configure(
            stderr="the classifier will not load: kaboom\n", exit=1
        )
        with pytest.raises(ClassifyOutputError) as raised:
            classify_document(
                repo / "tests" / "formats" / "ACME-EN24" / "report.pdf", repo
            )
        assert "kaboom" in str(raised.value)


class TestWhenTheEngineCannotAnswer:
    def test_a_classifier_that_does_not_load_fails_the_test_with_the_engines_words(
        self, pytester, fake_freeports
    ):
        make_repo(pytester.path, {"ACME-EN24": CLASSIFIED_ACME_EN24})
        fake_freeports.configure(
            stderr="the classifier will not load: kaboom\n", exit=1
        )
        result = run_suite(pytester)
        result.assert_outcomes(failed=1)
        assert "kaboom" in result.stdout.str()

    def test_a_classification_error_fails_the_test(self, pytester, fake_freeports):
        make_repo(pytester.path, {"ACME-EN24": CLASSIFIED_ACME_EN24})
        fake_freeports.configure(
            stdout="report: error: page 1 could not be read: boom\n", exit=3
        )
        result = run_suite(pytester)
        result.assert_outcomes(failed=1)
        assert "page 1 could not be read: boom" in result.stdout.str()

    def test_the_plugin_classifies_through_the_command(self, pytester, fake_freeports):
        """One `freeports classify` per document collected, and its answer is what is judged."""
        make_repo(
            pytester.path,
            {"ACME-EN24": UNCLASSIFIED, "BETA-EN24": UNCLASSIFIED},
        )
        fake_freeports.configure(stdout="report: ACME-EN24\n")
        result = run_suite(pytester)
        result.assert_outcomes(passed=1, failed=1)
        assert len(fake_freeports.calls()) == 2
