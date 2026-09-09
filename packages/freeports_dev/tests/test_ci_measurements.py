"""The language measurements: docstrings, lint scores, and reading each tool's own dialect.

Three commands that establish facts and decide nothing. What is worth pinning about them is not
that they can run a tool — anybody can run a tool — but the handful of judgements they encode,
each of which would be invisible in a Makefile recipe and each of which would quietly move a
threshold if it changed.

**Nothing here runs cargo, ruff or pytest.** Every reader is exercised against a fixture in the
tool's own shape, which is what lets these tests pin the parsing on a machine where the tool is not
installed — and, more to the point, makes the suite say so the day a tool moves a key, instead of a
recipe producing an empty string, the gate reading it as zero, and a commit being refused for a
reason nobody can find.

The docstring metric is deliberately **not** sphinx's. `sphinx.ext.coverage` measures whether an
object appears in the built site, which is a fact about how autosummary is configured: every module
of the compiled `freeports` extension reports a vacuous 100 % because `inspect.getmembers`
attributes nothing to it. A threshold on that would move whenever somebody edited a template.
"""

import json

import pytest

from freeports_dev.ci import docstrings, lint, readers, report, suites
from freeports_dev.ci.readers import ReaderError


def module(tmp_path, name, text):
    path = tmp_path / f"{name}.py"
    path.write_text(text, encoding="utf-8")
    return path


class TestWhatCountsAsDocumentable:
    def test_a_module_itself_counts(self, tmp_path):
        found = docstrings.walk_module(
            module(tmp_path, "m", '"""Says what it is."""\n')
        )
        assert [(o.kind, o.documented) for o in found] == [("module", True)]

    def test_a_module_with_no_docstring_counts_and_is_undocumented(self, tmp_path):
        found = docstrings.walk_module(module(tmp_path, "m", "x = 1\n"))
        assert [(o.kind, o.documented) for o in found] == [("module", False)]

    def test_public_functions_classes_and_methods_count(self, tmp_path):
        source = '"""M."""\n\n\ndef f():\n    """F."""\n\n\nclass C:\n    """C."""\n\n    def m(self):\n        """M."""\n'
        found = docstrings.walk_module(module(tmp_path, "m", source))
        assert sorted(o.kind for o in found) == [
            "class",
            "function",
            "method",
            "module",
        ]
        assert all(o.documented for o in found)

    def test_a_private_name_counts_in_neither_direction(self, tmp_path):
        source = '"""M."""\n\n\ndef _helper():\n    pass\n'
        found = docstrings.walk_module(module(tmp_path, "m", source))
        assert [o.kind for o in found] == ["module"]

    def test_but_a_dunder_is_public_behaviour_in_a_private_spelling(self, tmp_path):
        source = '"""M."""\n\n\nclass C:\n    """C."""\n\n    def __init__(self):\n        pass\n'
        found = docstrings.walk_module(module(tmp_path, "m", source))
        assert any(o.kind == "method" and not o.documented for o in found)

    def test_a_nested_function_is_an_implementation_detail_of_its_parent(
        self, tmp_path
    ):
        """Counting it would let a module be dragged down by how somebody factored one body."""
        source = '"""M."""\n\n\ndef outer():\n    """O."""\n\n    def inner():\n        pass\n\n    return inner\n'
        found = docstrings.walk_module(module(tmp_path, "m", source))
        assert [o.kind for o in found] == ["module", "function"]

    def test_a_nested_class_is_not(self, tmp_path):
        source = '"""M."""\n\n\nclass Outer:\n    """O."""\n\n    class Inner:\n        pass\n'
        found = docstrings.walk_module(module(tmp_path, "m", source))
        assert sum(1 for o in found if o.kind == "class") == 2

    def test_an_async_function_counts_like_any_other(self, tmp_path):
        source = '"""M."""\n\n\nasync def f():\n    pass\n'
        found = docstrings.walk_module(module(tmp_path, "m", source))
        assert any(o.kind == "function" and not o.documented for o in found)

    def test_the_qualified_name_reads_the_way_an_import_does(self, tmp_path):
        source = (
            '"""M."""\n\n\nclass C:\n    """C."""\n\n    def m(self):\n        pass\n'
        )
        found = docstrings.walk_module(module(tmp_path, "m", source), "pkg.mod")
        assert {o.qualified_name for o in found} == {
            "pkg.mod",
            "pkg.mod.C",
            "pkg.mod.C.m",
        }


class TestTheDocstringRatio:
    def test_it_is_documented_over_documentable(self, tmp_path):
        module(tmp_path, "a", '"""A."""\n\n\ndef f():\n    pass\n')
        coverage, measurement = docstrings.measure(tmp_path)
        assert coverage.total == 2
        assert coverage.documented == 1
        assert measurement.value == 50.0

    def test_a_root_with_nothing_public_in_it_reads_a_hundred(self, tmp_path):
        """There is nothing in it anybody failed to write about."""
        coverage, _ = docstrings.measure(tmp_path)
        assert coverage.ratio == 100.0

    def test_what_is_undocumented_is_named_with_its_line(self, tmp_path):
        module(tmp_path, "a", '"""A."""\n\n\ndef f():\n    pass\n')
        _, measurement = docstrings.measure(tmp_path)
        assert any(
            "a.f" in entry and ":4" in entry
            for entry in measurement.detail["undocumented"]
        )

    def test_a_file_that_does_not_parse_is_a_finding_not_a_crash(self, tmp_path):
        module(tmp_path, "broken", "def (:\n")
        module(tmp_path, "fine", '"""Fine."""\n')
        coverage, measurement = docstrings.measure(tmp_path)
        assert coverage.ratio == 100.0
        assert measurement.detail["unreadable"]

    def test_build_output_and_caches_are_not_somebody_s_source(self, tmp_path):
        (tmp_path / "build").mkdir()
        (tmp_path / "build" / "stale.py").write_text("def f():\n    pass\n")
        module(tmp_path, "real", '"""Real."""\n')
        coverage, _ = docstrings.measure(tmp_path)
        assert coverage.total == 1

    def test_an_init_takes_its_package_name_rather_than_becoming_init(self, tmp_path):
        (tmp_path / "pkg").mkdir()
        (tmp_path / "pkg" / "__init__.py").write_text('"""P."""\n')
        assert (
            docstrings.module_name_for(tmp_path / "pkg" / "__init__.py", tmp_path)
            == "pkg"
        )

    def test_the_breakdown_is_per_module(self, tmp_path):
        module(tmp_path, "a", '"""A."""\n')
        module(tmp_path, "b", "x = 1\n")
        _, measurement = docstrings.measure(tmp_path)
        assert measurement.breakdown == {"a": 100.0, "b": 0.0}


class TestTheLintScore:
    """Pylint's formula, so the number means what it meant on the trend graph this project plotted."""

    def test_a_clean_codebase_scores_ten(self):
        assert lint.score(0, 0, 1000) == 10.0

    def test_a_warning_costs_a_tenth_of_a_point_per_percent_of_the_codebase(self):
        assert lint.score(0, 1, 1000) == pytest.approx(9.99)

    def test_an_error_weighs_five_warnings(self):
        assert lint.score(1, 0, 1000) == lint.score(0, 5, 1000)

    def test_the_score_is_floored_at_zero_rather_than_going_negative(self):
        assert lint.score(1000, 0, 10) == 0.0

    def test_an_empty_file_set_scores_ten_rather_than_dividing_by_zero(self):
        assert lint.score(0, 0, 0) == 10.0

    def test_it_reproduces_the_measured_baseline_of_the_formats_repository(self):
        """97 ruff violations over 2 908 lines of `content/`, which read 9.67 when first measured."""
        assert lint.score(0, 97, 2908) == pytest.approx(9.67, abs=0.005)

    def test_and_the_one_of_the_crate(self):
        """8 clippy warnings over 44 617 lines, which read 9.998."""
        assert lint.score(0, 8, 44617) == pytest.approx(9.998, abs=0.0005)

    def test_the_same_count_scores_higher_in_a_bigger_codebase(self):
        """Which is the whole reason the metric is a score and not a count."""
        assert lint.score(0, 8, 44617) > lint.score(0, 8, 2908)


class TestCountingTheDenominator:
    def test_blank_lines_are_not_statements(self, tmp_path):
        path = module(tmp_path, "a", "x = 1\n\n\ny = 2\n")
        assert lint.count_statements([path]) == 2

    def test_whole_line_comments_are_not_statements(self, tmp_path):
        path = module(tmp_path, "a", "# a note\nx = 1\n")
        assert lint.count_statements([path]) == 1

    def test_a_trailing_comment_sits_on_a_line_that_is_one(self, tmp_path):
        path = module(tmp_path, "a", "x = 1  # a note\n")
        assert lint.count_statements([path]) == 1

    def test_rust_line_comments_are_not_statements_either(self, tmp_path):
        path = tmp_path / "a.rs"
        path.write_text("// a note\nlet x = 1;\n")
        assert lint.count_statements([path]) == 1

    def test_docstrings_are_counted_on_purpose(self, tmp_path):
        """Excluding them would make the lint score rise as documentation is deleted."""
        path = module(tmp_path, "a", '"""One."""\nx = 1\n')
        assert lint.count_statements([path]) == 2

    def test_a_file_that_cannot_be_read_is_skipped_rather_than_fatal(self, tmp_path):
        assert lint.count_statements([tmp_path / "gone.py"]) == 0


class TestReadingRuff:
    def test_every_violation_weighs_one(self):
        payload = json.dumps(
            [{"code": "F401", "filename": "a.py"}, {"code": "E741", "filename": "a.py"}]
        )
        result = lint.parse_ruff(payload, paths=[])
        assert (result.errors, result.warnings) == (0, 2)

    def test_a_clean_report_is_no_findings_rather_than_a_failure(self):
        assert lint.parse_ruff("[]", paths=[]).total == 0

    def test_the_rules_are_counted_so_somebody_knows_where_to_start(self):
        payload = json.dumps([{"code": "F401"}, {"code": "F401"}, {"code": "E741"}])
        assert lint.parse_ruff(payload, paths=[]).by_rule() == {"F401": 2, "E741": 1}

    def test_a_violation_with_no_code_is_still_counted(self):
        assert lint.parse_ruff(json.dumps([{}]), paths=[]).warnings == 1


class TestReadingClippy:
    def message(self, level, code="clippy::redundant_closure"):
        return json.dumps(
            {
                "reason": "compiler-message",
                "message": {
                    "level": level,
                    "code": {"code": code} if code else None,
                    "spans": [{"is_primary": True}],
                },
            }
        )

    def test_warnings_and_errors_are_told_apart(self):
        result = lint.parse_clippy(
            [self.message("warning"), self.message("error")], 1000
        )
        assert (result.errors, result.warnings) == (1, 1)

    def test_records_that_are_not_diagnostics_are_ignored(self):
        lines = [json.dumps({"reason": "compiler-artifact"}), self.message("warning")]
        assert lint.parse_clippy(lines, 1000).total == 1

    def test_the_summary_line_would_double_every_figure_and_is_dropped(self):
        """Cargo emits "generated 8 warnings" as a diagnostic with no code and no primary span."""
        summary = json.dumps(
            {
                "reason": "compiler-message",
                "message": {"level": "warning", "code": None, "spans": []},
            }
        )
        assert lint.parse_clippy([self.message("warning"), summary], 1000).total == 1

    def test_a_note_is_not_a_finding(self):
        note = json.dumps(
            {"reason": "compiler-message", "message": {"level": "note", "spans": []}}
        )
        assert lint.parse_clippy([note], 1000).total == 0

    def test_a_line_that_is_not_json_is_stepped_over(self):
        lines = ["   Compiling freeports v0.1.0", self.message("warning")]
        assert lint.parse_clippy(lines, 1000).total == 1


class TestReadingLlvmCov:
    def report(self, percent=91.18, covered=28548, count=31311):
        return json.dumps(
            {
                "data": [
                    {
                        "totals": {
                            "lines": {
                                "percent": percent,
                                "covered": covered,
                                "count": count,
                            }
                        },
                        "files": [
                            {
                                "filename": "src/a.rs",
                                "summary": {"lines": {"percent": 80.0}},
                            }
                        ],
                    }
                ]
            }
        )

    def test_the_line_figure_is_the_one_taken(self):
        measurement = readers.from_llvm_cov(self.report())
        assert measurement.value == pytest.approx(91.18)
        assert measurement.unit == "percent"

    def test_the_counts_come_with_it(self):
        assert readers.from_llvm_cov(self.report()).detail == {
            "covered": 28548,
            "total": 31311,
        }

    def test_the_per_file_breakdown_is_kept(self):
        assert readers.from_llvm_cov(self.report()).breakdown == {"src/a.rs": 80.0}

    def test_a_file_that_is_not_an_llvm_cov_report_says_so(self):
        with pytest.raises(ReaderError):
            readers.from_llvm_cov(json.dumps({"totals": {}}))


class TestReadingCoveragePy:
    def report(self):
        return json.dumps(
            {
                "totals": {
                    "percent_covered": 27.69,
                    "covered_lines": 438,
                    "num_statements": 1582,
                },
                "files": {"a.py": {"summary": {"percent_covered": 50.0}}},
            }
        )

    def test_the_figure_is_recomputed_so_it_matches_its_own_counts(self):
        """coverage.py's own `percent_covered` is rounded; 438/1582 is 27.6865, not 27.69.

        Recomputing costs nothing and makes the number the report prints agree with the
        `covered/total` printed beside it, so a person checking it by hand gets the same answer.
        """
        measurement = readers.from_coverage_py(self.report())
        assert measurement.value == pytest.approx(100 * 438 / 1582)
        assert measurement.detail == {"covered": 438, "total": 1582}

    def test_the_per_file_breakdown_is_kept(self):
        assert readers.from_coverage_py(self.report()).breakdown == {"a.py": 50.0}

    def test_something_else_entirely_says_so(self):
        with pytest.raises(ReaderError):
            readers.from_coverage_py(json.dumps({"data": []}))

    def test_several_reports_are_combined_over_their_counts_not_averaged(self):
        """Averaging would give a fifty-line package the same weight as a five-thousand-line one."""
        small = json.dumps(
            {
                "totals": {
                    "percent_covered": 100.0,
                    "covered_lines": 10,
                    "num_statements": 10,
                }
            }
        )
        large = json.dumps(
            {
                "totals": {
                    "percent_covered": 0.0,
                    "covered_lines": 0,
                    "num_statements": 990,
                }
            }
        )
        measurement = readers.from_coverage_py([small, large])
        assert measurement.detail == {"covered": 10, "total": 1000}
        assert measurement.value == pytest.approx(1.0)

    def test_and_the_averaged_answer_would_have_been_fifty(self):
        """Pinned so the difference between the two is visible rather than merely asserted."""
        small = json.dumps(
            {
                "totals": {
                    "percent_covered": 100.0,
                    "covered_lines": 10,
                    "num_statements": 10,
                }
            }
        )
        large = json.dumps(
            {
                "totals": {
                    "percent_covered": 0.0,
                    "covered_lines": 0,
                    "num_statements": 990,
                }
            }
        )
        assert readers.from_coverage_py([small, large]).value != 50.0

    def test_a_package_with_no_statements_in_it_does_not_divide_by_zero(self):
        empty = json.dumps(
            {
                "totals": {
                    "percent_covered": 0.0,
                    "covered_lines": 0,
                    "num_statements": 0,
                }
            }
        )
        assert readers.from_coverage_py(empty).value == 100.0


class TestReadingRustdoc:
    def report(self):
        return json.dumps(
            {
                "src/a.rs": {"total": 10, "with_docs": 4},
                "src/b.rs": {"total": 10, "with_docs": 6},
                "#ALL#": {"total": 20, "with_docs": 10},
            }
        )

    def test_the_total_row_is_a_total_and_is_not_counted_as_a_file(self):
        """Counting `#ALL#` would double every item and halve nothing, which is worse than either."""
        measurement = readers.from_rustdoc(self.report())
        assert measurement.detail == {"documented": 10, "total": 20}
        assert "#ALL#" not in measurement.breakdown

    def test_the_percentage_is_recomputed_so_it_matches_its_own_counts(self):
        assert readers.from_rustdoc(self.report()).value == 50.0

    def test_the_per_file_breakdown_is_kept(self):
        assert readers.from_rustdoc(self.report()).breakdown == {
            "src/a.rs": 40.0,
            "src/b.rs": 60.0,
        }

    def test_a_report_with_no_items_in_it_is_not_a_hundred_percent(self):
        with pytest.raises(ReaderError):
            readers.from_rustdoc(json.dumps({}))

    def test_something_that_is_not_a_report_at_all(self):
        with pytest.raises(ReaderError):
            readers.from_rustdoc(json.dumps([1, 2, 3]))


class TestTheMeasurementRecord:
    def test_it_round_trips_through_its_own_json(self, tmp_path):
        original = report.Measurement(
            "docs.rust", 39.4, "percent", head="abc", breakdown={"a": 1.0}
        )
        path = original.write(tmp_path / report.file_name("docs.rust"))
        restored = report.Measurement.from_dict(json.loads(path.read_text()))
        assert (restored.metric, restored.value, restored.head) == (
            "docs.rust",
            39.4,
            "abc",
        )

    def test_the_file_name_is_mechanical(self):
        assert report.file_name("tests.python.lines") == "tests-python-lines.json"

    def test_an_unmeasured_metric_is_not_a_value_of_zero(self):
        """A figure nobody could compute is not a figure above the threshold."""
        measurement = report.Measurement.unmeasured(
            "tests.rust.lines", "cargo-llvm-cov is missing"
        )
        assert measurement.value is None
        assert not measurement.is_measured
        assert "cargo-llvm-cov" in measurement.reason

    def test_reading_a_directory_of_them_keys_by_metric(self, tmp_path):
        report.Measurement("docs.rust", 39.4, "percent").write(
            tmp_path / "docs-rust.json"
        )
        report.Measurement("lint.rust", 9.9, "score").write(tmp_path / "lint-rust.json")
        found = report.read_all(tmp_path)
        assert set(found) == {"docs.rust", "lint.rust"}

    def test_one_unparseable_file_does_not_stop_the_other_eleven(self, tmp_path):
        report.Measurement("docs.rust", 39.4, "percent").write(
            tmp_path / "docs-rust.json"
        )
        (tmp_path / "broken.json").write_text("{not json")
        assert set(report.read_all(tmp_path)) == {"docs.rust"}

    def test_a_directory_that_is_not_there_holds_no_measurements(self, tmp_path):
        assert report.read_all(tmp_path / "nothing") == {}


class TestTheSuiteRecord:
    """A suite's outcome is a fact about a commit, stored the way a measurement is.

    The point of storing it rather than announcing it on a command line: a claim typed into
    `ci-check --suite fast:passed` describes one run and is silent about every suite that did not
    happen, and that silence is what a green tick was being drawn from.
    """

    def test_it_round_trips_through_its_own_json(self, tmp_path):
        original = report.SuiteOutcome("rust.unit", "passed", head="abc")
        path = original.write(tmp_path / report.suite_file_name("rust.unit"))
        restored = report.SuiteOutcome.from_dict(json.loads(path.read_text()))
        assert (restored.suite, restored.outcome, restored.head) == (
            "rust.unit",
            "passed",
            "abc",
        )

    def test_the_file_name_is_mechanical_and_prefixed(self, tmp_path):
        assert report.suite_file_name("python.fast") == "suite-python-fast.json"

    def test_reading_a_directory_of_them_keys_by_suite(self, tmp_path):
        report.SuiteOutcome("rust.unit", "passed").write(
            tmp_path / "suite-rust-unit.json"
        )
        report.SuiteOutcome("python.fast", "failed").write(
            tmp_path / "suite-python-fast.json"
        )
        found = report.read_suites(tmp_path)
        assert set(found) == {"rust.unit", "python.fast"}
        assert found["rust.unit"].passed
        assert not found["python.fast"].passed

    def test_measurements_and_suites_share_a_directory_without_reading_each_other(
        self, tmp_path
    ):
        """Told apart by the key each carries, so a file somebody renamed still reads as itself."""
        report.Measurement("docs.rust", 39.4, "percent").write(
            tmp_path / "docs-rust.json"
        )
        report.SuiteOutcome("rust.unit", "passed").write(
            tmp_path / "suite-rust-unit.json"
        )
        assert set(report.read_all(tmp_path)) == {"docs.rust"}
        assert set(report.read_suites(tmp_path)) == {"rust.unit"}

    def test_one_unparseable_file_does_not_hide_the_others(self, tmp_path):
        report.SuiteOutcome("rust.unit", "passed").write(
            tmp_path / "suite-rust-unit.json"
        )
        (tmp_path / "suite-broken.json").write_text("{not json")
        assert set(report.read_suites(tmp_path)) == {"rust.unit"}

    def test_a_directory_that_is_not_there_holds_no_outcomes(self, tmp_path):
        assert report.read_suites(tmp_path / "nothing") == {}


class TestTheSuiteRegistry:
    """What is in the commit gate, pinned. Moving a suite across this line changes every commit."""

    def test_the_fast_suites_are_the_ones_a_commit_can_pay_for(self):
        fast = {s.name for s in suites.REGISTRY if s.cost == suites.FAST}
        assert fast == {"rust.unit", "python.fast", "formats.single_page"}

    def test_and_everything_else_is_run_deliberately(self):
        slow = {s.name for s in suites.REGISTRY if s.cost == suites.SLOW}
        assert slow == {
            "rust.integration",
            "rust.doc",
            "python.slow",
            "python.online",
            "formats.integration",
        }

    def test_an_online_suite_is_slow_even_though_it_is_quick(self):
        """Two categories, one policy. It is out of the gate because of whose machine it needs."""
        online = suites.find("python.online")
        assert online.online
        assert online.cost == suites.SLOW
        assert "network" in online.expense

    def test_a_slow_one_that_is_not_online_says_so_differently(self):
        assert "seconds" in suites.find("rust.integration").expense

    def test_a_fast_suite_has_no_excuse_to_offer(self):
        assert suites.find("rust.unit").expense is None

    def test_every_suite_names_the_command_that_runs_it(self):
        """A verdict that says `stale` without saying how to fix it is a verdict people route
        around."""
        assert all(s.command for s in suites.REGISTRY)

    def test_a_repository_is_only_asked_about_the_suites_it_has(self):
        engine = {s.name for s in suites.known_in(suites.ENGINE)}
        formats = {s.name for s in suites.known_in(suites.FORMATS)}
        assert "rust.doc" in engine and "formats.integration" not in engine
        assert "formats.integration" in formats and "rust.doc" not in formats

    def test_an_unknown_name_belongs_to_no_suite(self):
        assert suites.find("rust.made_up") is None


class TestReadingCheckKeys:
    """A key nobody could look up makes the metric unmeasured, never a lower number."""

    def test_every_key_published_is_a_hundred(self):
        measurement = readers.from_check_keys(
            "published 2/2, not published 0, could not be checked 0"
        )
        assert measurement.value == 100.0
        assert measurement.is_measured

    def test_none_published_is_zero_and_is_a_measurement(self):
        measurement = readers.from_check_keys(
            "published 0/1, not published 1, could not be checked 0"
        )
        assert measurement.value == 0.0
        assert measurement.is_measured

    def test_a_key_that_could_not_be_looked_up_makes_it_unmeasured(self):
        """Reporting it as a number would turn a fact about the network into one about the repo."""
        measurement = readers.from_check_keys(
            "published 1/2, not published 0, could not be checked 1"
        )
        assert not measurement.is_measured
        assert "nobody looked" in measurement.reason

    def test_a_repository_with_no_granter_has_no_unpublished_key_in_it(self):
        measurement = readers.from_check_keys(
            "published 0/0, not published 0, could not be checked 0"
        )
        assert measurement.value == 100.0

    def test_something_that_is_not_a_summary_says_so(self):
        with pytest.raises(ReaderError):
            readers.from_check_keys("all fine")
