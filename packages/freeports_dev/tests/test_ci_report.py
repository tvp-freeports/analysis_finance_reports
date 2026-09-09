"""The report: the gate run written down, in the five shapes it has to go in.

`freeports-dev ci-report` renders what `ci-check` decided. It measures nothing and it judges
nothing, so what is tested here is entirely about *rendering faithfully*: that the model says what
the gate said, that a status word cannot flatter a run, that a badge cannot colour a shortfall
green, and that a rendering written into a file somebody else wrote goes where they said and
nowhere else.

Three of these would be silently broken by a plausible change, and they are the reason this module
exists:

**`inconclusive` is never `passing`.** A figure nobody could compute is not a figure above the
threshold — the rule the gate already applies — and a report that rounded it up to `passing` would
undo that in the one place a stranger actually reads.

**No rendering names a commit unless something is stale.** These artefacts are committed. A date,
or the head recorded beside every figure, puts a diff in every commit saying nothing changed, and
the pressure to then stop committing them at all is how a published figure goes a year out of date.

**A badge is coloured by the figure, not by the verdict.** Painting 32 % green because 32 is what
this repository currently demands would flatter a reader with the project's own floor.
"""

import json
import subprocess
from argparse import Namespace

import pytest

from freeports_dev.ci import gate as ci_gate
from freeports_dev.ci import render
from freeports_dev.ci import report
from freeports_dev.ci.config import CiConfig


def args(**overrides):
    return Namespace(
        **{
            "repo": None,
            "branch_class": None,
            "min": None,
            "keyserver": None,
            "breakdown_limit": None,
            **overrides,
        }
    )


@pytest.fixture(autouse=True)
def no_ambient_environment(monkeypatch):
    import os

    for name in list(os.environ):
        if name.startswith("FREEPORTS_CI_") or name == "FREEPORTS_VALIDATE_KEYSERVER":
            monkeypatch.delenv(name, raising=False)


@pytest.fixture
def engine(tmp_path):
    """An engine-shaped repository on a `dev` branch."""
    (tmp_path / "packages" / "freeports").mkdir(parents=True)
    subprocess.run(["git", "-C", str(tmp_path), "init", "-q", "-b", "dev"], check=True)
    return tmp_path


def measured(metric, value, head=None, unit="percent", breakdown=None):
    return report.Measurement(metric, value, unit, head=head, breakdown=breakdown)


def model_of(
    root,
    thresholds,
    measurements,
    branch_class=None,
    head=None,
    conditions=None,
    skipped_slow=None,
):
    (root / "ci.yaml").write_text(thresholds)
    config = CiConfig(root, args(branch_class=branch_class))
    gate = ci_gate.evaluate(
        config, measurements, conditions, head=head, skipped_slow=skipped_slow
    )
    return render.build(gate, config)


def entry_for(model, metric):
    return next(e for e in model["metrics"] if e["metric"] == metric)


ONE_THRESHOLD = "thresholds:\n  docs.rust: 39\n"


class TestTheModel:
    """What the gate decided, plus what the registry knows about each metric."""

    def test_it_carries_the_verdict_the_gate_reached(self, engine):
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        assert entry_for(model, "docs.rust")["verdict"] == ci_gate.PASS

    def test_it_carries_what_the_metric_measures_and_what_it_costs(self, engine):
        """A table reading `docs.rust 39.4 %` tells a reader a number and not what was counted."""
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        entry = entry_for(model, "docs.rust")
        assert entry["cost"] == "fast"
        assert "rustdoc" in entry["description"]

    def test_it_names_the_branch_and_the_rule_that_classified_it(self, engine):
        model = model_of(engine, ONE_THRESHOLD, {})
        assert model["branch"]["name"] == "dev"
        assert model["branch"]["class"] == "dev"
        assert model["branch"]["reason"]
        assert model["branch"]["enforced"] is False

    def test_the_totals_add_up_to_the_metrics(self, engine):
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        totals = model["totals"]
        counted = sum(
            totals[key] for key in ("pass", "below", "unmeasured", "stale", "ungated")
        )
        assert counted == totals["metrics"] == len(model["metrics"])

    def test_a_breakdown_path_loses_the_machine_it_was_measured_on(self, engine):
        """An absolute path names somebody's home directory in a file the repository commits."""
        inside = str(engine / "packages" / "freeports" / "src" / "run.rs")
        model = model_of(
            engine,
            ONE_THRESHOLD,
            {"docs.rust": measured("docs.rust", 39.4, breakdown={inside: 12.0})},
        )
        assert list(entry_for(model, "docs.rust")["breakdown"]) == [
            "packages/freeports/src/run.rs"
        ]

    def test_a_breakdown_key_that_is_not_a_path_is_left_alone(self, engine):
        model = model_of(
            engine,
            "thresholds:\n  lint.rust: 9\n",
            {
                "lint.rust": measured(
                    "lint.rust", 9.9, unit="score", breakdown={"clippy::ptr_arg": 1}
                )
            },
        )
        assert list(entry_for(model, "lint.rust")["breakdown"]) == ["clippy::ptr_arg"]


class TestTheStatusWord:
    """One word for the whole run, and it may never be kinder than what the gate found."""

    def test_everything_measured_and_over_its_minimum_is_passing(self, engine):
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        assert model["status"] == render.PASSING

    def test_a_figure_under_its_minimum_is_failing(self, engine):
        model = model_of(
            engine,
            "thresholds:\n  docs.rust: 40\n",
            {"docs.rust": measured("docs.rust", 39.4)},
        )
        assert model["status"] == render.FAILING

    def test_a_figure_nobody_could_compute_is_inconclusive_and_never_passing(
        self, engine
    ):
        model = model_of(engine, ONE_THRESHOLD, {})
        assert model["status"] == render.INCONCLUSIVE
        assert model["status"] != render.PASSING

    def test_a_figure_taken_at_another_commit_is_inconclusive(self, engine):
        model = model_of(
            engine,
            ONE_THRESHOLD,
            {"docs.rust": measured("docs.rust", 39.4, head="a" * 40)},
            head="b" * 40,
        )
        assert model["status"] == render.INCONCLUSIVE

    def test_a_shortfall_outranks_an_unmeasured_figure(self, engine):
        """`failing` is the strongest thing a run can say, and one unknown does not soften it."""
        model = model_of(
            engine,
            "thresholds:\n  docs.rust: 40\n  docs.python: 50\n",
            {"docs.rust": measured("docs.rust", 39.4)},
        )
        assert model["status"] == render.FAILING

    def test_a_suite_that_did_not_pass_is_failing_even_with_every_figure_clear(
        self, engine
    ):
        model = model_of(
            engine,
            ONE_THRESHOLD,
            {"docs.rust": measured("docs.rust", 39.4)},
            conditions=[ci_gate.parse_suite("fast:failed")],
        )
        assert model["status"] == render.FAILING

    def test_a_metric_skipped_by_skip_slow_is_inconclusive_not_passing(self, engine):
        """A figure nobody looked at must not be mistaken afterwards for one that passed."""
        model = model_of(
            engine,
            ONE_THRESHOLD,
            {"docs.rust": measured("docs.rust", 39.4)},
            skipped_slow=["tests.rust.lines"],
        )
        assert model["status"] == render.INCONCLUSIVE

    def test_a_run_that_gated_nothing_is_inconclusive_rather_than_vacuously_passing(
        self, engine
    ):
        """A green badge on a repository nobody has configured is the flattery to keep out."""
        model = model_of(engine, "", {"docs.rust": measured("docs.rust", 39.4)})
        assert model["totals"]["gated"] == 0
        assert model["status"] == render.INCONCLUSIVE

    def test_the_status_is_not_the_branch_class(self, engine):
        """A dev branch refuses nothing, and the run is still failing. Two different questions."""
        model = model_of(
            engine,
            "thresholds:\n  docs.rust: 40\n",
            {"docs.rust": measured("docs.rust", 39.4)},
            branch_class="dev",
        )
        assert model["status"] == render.FAILING
        assert model["refused"] is False


class TestWhatNoRenderingSays:
    def test_no_commit_is_named_when_nothing_is_stale(self, engine):
        """Otherwise every commit rewrites four committed files to say nothing changed."""
        head = "b" * 40
        model = model_of(
            engine,
            ONE_THRESHOLD,
            {"docs.rust": measured("docs.rust", 39.4, head=head)},
            head=head,
        )
        for style in ("markdown", "rst", "html"):
            assert head[:8] not in render.render(model, style)

    def test_a_stale_figure_does_name_the_commit_it_was_taken_at(self, engine):
        """There it is the news, and hiding it would make the fast/slow split dishonest."""
        model = model_of(
            engine,
            ONE_THRESHOLD,
            {"docs.rust": measured("docs.rust", 39.4, head="a" * 40)},
            head="b" * 40,
        )
        assert "aaaaaaaa" in render.render(model, "markdown")

    def test_nothing_carries_a_date(self, engine):
        import datetime

        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        year = str(datetime.date.today().year)
        for style in ("markdown", "rst", "html"):
            assert year not in render.render(model, style)

    def test_every_rendering_says_it_is_demonstrative(self, engine):
        """A page of percentages invites exactly one misreading, and this is what refuses it."""
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        for style in ("markdown", "rst", "html"):
            text = render.render(model, style)
            assert "demonstrative" in text
            assert render.GATE_DOC_URL in text


class TestTheTables:
    def test_the_summary_lists_every_metric(self, engine):
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        text = render.render(model, "markdown", "summary")
        for entry in model["metrics"]:
            assert entry["metric"] in text

    def test_the_thresholds_table_lists_only_what_has_a_minimum(self, engine):
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        text = render.render(model, "markdown", "thresholds")
        assert "docs.rust" in text
        assert "lint.rust" not in text

    def test_the_breakdown_is_worst_first_for_a_percentage(self, engine):
        model = model_of(
            engine,
            ONE_THRESHOLD,
            {
                "docs.rust": measured(
                    "docs.rust", 39.4, breakdown={"good.rs": 90.0, "bad.rs": 10.0}
                )
            },
        )
        text = render.render(model, "markdown", "breakdown")
        assert text.index("bad.rs") < text.index("good.rs")

    def test_the_breakdown_is_worst_first_for_a_count_of_findings(self, engine):
        """The one quantity here where a bigger number is a worse repository."""
        model = model_of(
            engine,
            "thresholds:\n  lint.rust: 9\n",
            {
                "lint.rust": measured(
                    "lint.rust", 9.9, unit="score", breakdown={"rare": 1, "common": 40}
                )
            },
        )
        text = render.render(model, "markdown", "breakdown")
        assert text.index("common") < text.index("rare")

    def test_a_yes_or_no_breakdown_lists_only_what_answers_for_something(self, engine):
        """Thirty-one documents that do have a test push the five that do not off the page."""
        model = model_of(
            engine,
            ONE_THRESHOLD,
            {
                "docs.rust": measured(
                    "docs.rust", 39.4, breakdown={"has": True, "lacks": False}
                )
            },
        )
        text = render.render(model, "markdown", "breakdown")
        assert "lacks" in text
        assert "has" not in text

    def test_a_list_valued_breakdown_is_counted_rather_than_formatted_as_a_number(
        self, engine
    ):
        model = model_of(
            engine,
            ONE_THRESHOLD,
            {"docs.rust": measured("docs.rust", 39.4, breakdown={"FORMAT": [13, 14]})},
        )
        assert "2 pages: 13, 14" in render.render(model, "markdown", "breakdown")

    def test_the_document_shows_the_worst_few_and_says_how_many_it_left_out(
        self, engine
    ):
        model = model_of(
            engine,
            ONE_THRESHOLD,
            {
                "docs.rust": measured(
                    "docs.rust",
                    39.4,
                    breakdown={f"f{n}.rs": float(n) for n in range(40)},
                )
            },
        )
        model["breakdown_limit"] = 5
        text = render.render(model, "markdown", "breakdown")
        assert "35 more" in text
        assert "f0.rs" in text and "f39.rs" not in text

    def test_a_limit_of_zero_shows_all_of_them(self, engine):
        model = model_of(
            engine,
            ONE_THRESHOLD,
            {
                "docs.rust": measured(
                    "docs.rust",
                    39.4,
                    breakdown={f"f{n}.rs": float(n) for n in range(40)},
                )
            },
        )
        model["breakdown_limit"] = 0
        text = render.render(model, "markdown", "breakdown")
        assert "f39.rs" in text
        assert render.breakdown_omitted(model) == []

    def test_the_model_keeps_every_line_whatever_a_document_shows(self, engine):
        """`--format json` is the complete answer, and it is the one that does not go in a commit."""
        model = model_of(
            engine,
            ONE_THRESHOLD,
            {
                "docs.rust": measured(
                    "docs.rust",
                    39.4,
                    breakdown={f"f{n}.rs": float(n) for n in range(40)},
                )
            },
        )
        reread = json.loads(render.render_json(model))
        assert len(entry_for(reread, "docs.rust")["breakdown"]) == 40


class TestTheBadges:
    def one(self, model, metric):
        return render.metric_badge(entry_for(model, metric))

    def test_the_run_gets_one_badge_carrying_its_status(self, engine):
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        endpoint = json.loads(render.render_badges(model)["ci-status.json"])
        assert endpoint["label"] == "ci"
        assert endpoint["message"] == render.PASSING

    def test_a_metric_that_is_neither_measured_nor_gated_gets_none(self, engine):
        """A wall of `n/a` badges says nothing and hides the ones that do."""
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        assert "lint-rust.svg" not in render.render_badges(model)

    def test_a_high_figure_is_green_and_a_low_one_is_not(self, engine):
        model = model_of(
            engine,
            "thresholds:\n  docs.rust: 10\n  docs.python: 10\n",
            {
                "docs.rust": measured("docs.rust", 95.0),
                "docs.python": measured("docs.python", 12.0),
            },
        )
        assert self.one(model, "docs.rust")[2] == "brightgreen"
        assert self.one(model, "docs.python")[2] == "red"

    def test_a_figure_clearing_a_low_minimum_is_not_painted_green_for_clearing_it(
        self, engine
    ):
        """The colour is the figure. A repository's own floor must not flatter its README."""
        model = model_of(
            engine,
            "thresholds:\n  docs.rust: 30\n",
            {"docs.rust": measured("docs.rust", 32.0)},
        )
        assert entry_for(model, "docs.rust")["verdict"] == ci_gate.PASS
        assert self.one(model, "docs.rust")[2] not in ("green", "brightgreen")

    def test_a_figure_under_its_minimum_is_red_whatever_its_value(self, engine):
        model = model_of(
            engine,
            "thresholds:\n  docs.rust: 99\n",
            {"docs.rust": measured("docs.rust", 95.0)},
        )
        assert self.one(model, "docs.rust")[2] == "red"

    def test_a_figure_nobody_could_compute_is_grey_and_says_so(self, engine):
        model = model_of(engine, ONE_THRESHOLD, {})
        label, message, color = self.one(model, "docs.rust")
        assert color == "grey"
        assert message == "not measured"

    def test_a_stale_figure_is_grey_and_does_not_show_its_number(self, engine):
        """The number is about other code, and printing it would say otherwise."""
        model = model_of(
            engine,
            ONE_THRESHOLD,
            {"docs.rust": measured("docs.rust", 39.4, head="a" * 40)},
            head="b" * 40,
        )
        label, message, color = self.one(model, "docs.rust")
        assert color == "grey"
        assert message == "stale"

    def test_every_badge_is_an_svg_and_an_endpoint(self, engine):
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        badges = render.render_badges(model)
        assert badges["docs-rust.svg"].lstrip().startswith("<svg")
        assert json.loads(badges["docs-rust.json"])["schemaVersion"] == 1

    def test_a_badge_file_is_named_after_its_metric(self, engine):
        assert render.badge_name("tests.python.freeports_dev.lines") == (
            "tests-python-freeports_dev-lines"
        )


class TestWritingIntoSomebodyElsesFile:
    def test_the_block_replaces_what_is_between_the_markers(self, engine):
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        block = render.render(model, "markdown")
        page = "# Title\n\n<!-- freeports-dev:begin -->\nstale text\n<!-- freeports-dev:end -->\n\nend\n"
        rewritten = render.rewrite(page, block)
        assert "stale text" not in rewritten
        assert rewritten.startswith("# Title")
        assert rewritten.rstrip().endswith("end")

    def test_it_is_idempotent(self, engine):
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        block = render.render(model, "markdown")
        page = "a\n<!-- freeports-dev:begin -->\n<!-- freeports-dev:end -->\nb\n"
        once = render.rewrite(page, block)
        assert render.rewrite(once, block) == once

    def test_a_file_with_no_markers_is_refused_rather_than_appended_to(self, engine):
        """Where a table belongs in a README is a judgement about that README."""
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        with pytest.raises(render.MarkersMissing):
            render.rewrite("# Just a README\n", render.render(model, "markdown"))

    def test_the_markers_name_their_writer(self, engine):
        """A README carries this block and the grants one; neither may overwrite the other."""
        for begin, end in render.MARKERS.values():
            assert "freeports-dev" in begin and "freeports-dev" in end
            assert "freeports-validate" not in begin

    def test_a_block_can_only_go_in_a_file_marked_for_its_own_format(self, engine):
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        rst = render.render(model, "rst")
        with pytest.raises(render.MarkersMissing):
            render.rewrite(
                "<!-- freeports-dev:begin -->\n<!-- freeports-dev:end -->\n", rst
            )


class TestWhatARenderingRefusesToDo:
    def test_json_has_no_tables_and_says_so_rather_than_ignoring_the_argument(
        self, engine
    ):
        model = model_of(engine, ONE_THRESHOLD, {})
        with pytest.raises(KeyError):
            render.render(model, "json", "breakdown")

    def test_html_carries_all_three_and_refuses_to_be_asked_for_one(self, engine):
        model = model_of(engine, ONE_THRESHOLD, {})
        with pytest.raises(KeyError):
            render.render(model, "html", "thresholds")

    def test_an_unknown_format_is_an_error(self, engine):
        model = model_of(engine, ONE_THRESHOLD, {})
        with pytest.raises(KeyError):
            render.render(model, "pdf")

    def test_an_unknown_table_is_an_error(self, engine):
        model = model_of(engine, ONE_THRESHOLD, {})
        with pytest.raises(KeyError):
            render.render(model, "markdown", "everything")


class TestTheHtmlPage:
    def test_it_is_self_contained(self, engine):
        """An air-gapped build and a laptop on a train render it the same as anybody else."""
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        page = render.render(model, "html")
        assert "<style>" in page and "<script>" in page
        assert "http://" not in page.replace("http://www.w3.org/2000/svg", "")

    def test_it_carries_the_three_arrangements_as_tabs(self, engine):
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        page = render.render(model, "html")
        for key, _label in render.VIEWPOINTS:
            assert f'data-panel="{key}"' in page

    def test_it_explains_only_the_verdicts_it_shows(self, engine):
        model = model_of(
            engine, ONE_THRESHOLD, {"docs.rust": measured("docs.rust", 39.4)}
        )
        page = render.render(model, "html")
        assert render.VERDICT_LEGEND[ci_gate.PASS] in page
        assert render.VERDICT_LEGEND[ci_gate.STALE] not in page

    def test_a_metric_name_cannot_carry_markup_into_the_page(self, engine):
        model = model_of(engine, ONE_THRESHOLD, {})
        model["metrics"][0]["metric"] = "<script>alert(1)</script>"
        assert "<script>alert(1)</script>" not in render.render(model, "html")


class TestTheThreeTiers:
    """`report.breakdown_limit`, like every other setting here: command line, environment, file."""

    def test_the_file_sets_it(self, engine):
        (engine / "ci.yaml").write_text("report:\n  breakdown_limit: 3\n")
        assert CiConfig(engine, args()).breakdown_limit == 3

    def test_the_environment_outranks_the_file(self, engine, monkeypatch):
        (engine / "ci.yaml").write_text("report:\n  breakdown_limit: 3\n")
        monkeypatch.setenv("FREEPORTS_CI_BREAKDOWN_LIMIT", "7")
        assert CiConfig(engine, args()).breakdown_limit == 7

    def test_the_command_line_outranks_both(self, engine, monkeypatch):
        (engine / "ci.yaml").write_text("report:\n  breakdown_limit: 3\n")
        monkeypatch.setenv("FREEPORTS_CI_BREAKDOWN_LIMIT", "7")
        assert CiConfig(engine, args(breakdown_limit="1")).breakdown_limit == 1

    def test_nothing_configured_falls_to_the_default(self, engine):
        (engine / "ci.yaml").write_text("")
        assert (
            CiConfig(engine, args()).breakdown_limit == render.DEFAULT_BREAKDOWN_LIMIT
        )

    def test_a_value_that_is_not_a_number_of_lines_is_an_error(self, engine):
        from freeports_dev.ci.config import ConfigError

        (engine / "ci.yaml").write_text("report:\n  breakdown_limit: many\n")
        with pytest.raises(ConfigError):
            CiConfig(engine, args()).breakdown_limit
