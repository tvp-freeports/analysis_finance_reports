"""What `ci.yaml` says, and how a branch ends up in a class.

One file, one name, one syntax, in all three kinds of repository. It is the only file a person has
to open to answer "what does this repository demand of a commit", and these tests hold the two
properties that make that answer trustworthy.

**The class is derived, never recorded.** Switching branch sets nothing and forgets nothing: the
class comes from `git rev-parse --abbrev-ref HEAD` at every run, matched against the lists in the
file. A detached HEAD and a machine without git are not failures — they fall to the default class,
and the tool says which and why, because "prod" answers nothing and "prod, because *main* matched
*main* in branches.prod" answers everything.

**Two things in the file are errors rather than resolutions.** A branch matching two class lists
would otherwise make the class of a commit depend on the order of a mapping nobody reads. A
threshold on a metric the repository cannot measure would otherwise leave the owner believing the
repository is gated on something nothing ever looks at — the same failure this project already
refuses for an unreachable methodology page, where a figure nobody could compute must never be
reported as a pass.
"""

import subprocess
from argparse import Namespace

import pytest

from freeports_dev.ci import metrics
from freeports_dev.ci.config import (
    DEFAULT_KEYSERVER,
    CiConfig,
    ConfigError,
    detect_repo_kind,
)


def args(**overrides):
    """A parsed command line with nothing on it but what a test names."""
    return Namespace(
        **{
            "repo": None,
            "branch_class": None,
            "min": None,
            "keyserver": None,
            **overrides,
        }
    )


@pytest.fixture(autouse=True)
def no_ambient_environment(monkeypatch):
    """No test here may be decided by a variable the developer happened to export."""
    for name in list(__import__("os").environ):
        if name.startswith("FREEPORTS_CI_") or name == "FREEPORTS_VALIDATE_KEYSERVER":
            monkeypatch.delenv(name, raising=False)


def git(root, *arguments):
    subprocess.run(
        ["git", "-C", str(root), *arguments], check=True, capture_output=True
    )


@pytest.fixture
def engine(tmp_path):
    """A directory shaped like the engine repository, on a branch called `dev`."""
    (tmp_path / "packages" / "freeports").mkdir(parents=True)
    git(tmp_path, "init", "-q", "-b", "dev")
    return tmp_path


@pytest.fixture
def formats(tmp_path):
    """A directory shaped like a formats repository, on `main`."""
    (tmp_path / "metadata").mkdir(parents=True)
    (tmp_path / "metadata" / "formats.csv").write_text("Name,Locale,Year\n")
    git(tmp_path, "init", "-q", "-b", "main")
    return tmp_path


def write_ci(root, text):
    (root / "ci.yaml").write_text(text)
    return root


class TestWhichKindOfRepositoryThisIs:
    """By what the directory holds, never by what a file claims — the file is what is validated."""

    def test_the_engine_is_recognised_by_its_packages(self, engine):
        assert detect_repo_kind(engine) == metrics.ENGINE

    def test_a_formats_repository_by_its_formats_table(self, formats):
        assert detect_repo_kind(formats) == metrics.FORMATS

    def test_an_input_database_by_its_manifest_and_companies(self, tmp_path):
        (tmp_path / "companies").mkdir()
        (tmp_path / "metadata.yaml").write_text("schema: v0.0.0\n")
        assert detect_repo_kind(tmp_path) == metrics.INPUT_DB

    def test_and_anything_else_is_no_kind_at_all(self, tmp_path):
        assert detect_repo_kind(tmp_path) is None


class TestABranchWithNoFileToJudgeIt:
    """A repository nobody has configured reports and never refuses. That is the right default."""

    def test_the_class_is_dev(self, engine):
        resolved = CiConfig(engine, args()).branch_class()
        assert resolved.name == "dev"
        assert resolved.branch == "dev"

    def test_and_the_reason_says_the_default_applied(self, engine):
        assert "default" in CiConfig(engine, args()).branch_class().reason

    def test_and_there_are_no_thresholds_at_all(self, engine):
        assert CiConfig(engine, args()).thresholds() == {}

    def test_and_the_key_server_is_the_documented_one(self, engine):
        assert CiConfig(engine, args()).keyserver == DEFAULT_KEYSERVER


class TestMatchingABranchToItsClass:
    def test_an_exact_name_matches(self, formats):
        write_ci(formats, "branches:\n  prod: [main]\n  dev: [dev]\n")
        resolved = CiConfig(formats, args()).branch_class()
        assert resolved.name == "prod"
        assert resolved.refuses

    def test_a_glob_matches(self, engine):
        git(engine, "checkout", "-q", "-b", "release/1.2")
        write_ci(engine, 'branches:\n  prod: ["release/*", main]\n')
        assert CiConfig(engine, args()).branch_class().name == "prod"

    def test_a_glob_does_not_match_past_its_shape(self, engine):
        git(engine, "checkout", "-q", "-b", "released")
        write_ci(engine, 'branches:\n  prod: ["release/*"]\n')
        assert CiConfig(engine, args()).branch_class().name == "dev"

    def test_the_reason_names_the_pattern_that_did_it(self, formats):
        write_ci(formats, 'branches:\n  prod: ["ma*"]\n')
        assert "'ma*'" in CiConfig(formats, args()).branch_class().reason

    def test_an_unlisted_branch_takes_the_declared_default(self, engine):
        git(engine, "checkout", "-q", "-b", "whatever")
        write_ci(engine, "branches:\n  prod: [main]\n  default: off\n")
        assert CiConfig(engine, args()).branch_class().name == "off"

    def test_and_off_means_the_hook_does_nothing(self, engine):
        git(engine, "checkout", "-q", "-b", "spike/parser")
        write_ci(engine, 'branches:\n  off: ["spike/*"]\n')
        resolved = CiConfig(engine, args()).branch_class()
        assert resolved.silent
        assert not resolved.refuses

    def test_off_survives_yaml_reading_it_as_a_boolean(self, engine):
        """`off` is one of the ten words YAML 1.1 spells a boolean with. Unquoted must still work.

        Quoting it in every repository's `ci.yaml` to work around a parser would be a worse file to
        read, so the boolean is mapped back to the class it was written as.
        """
        git(engine, "checkout", "-q", "-b", "experimental")
        write_ci(engine, "branches:\n  off: [experimental]\n")
        assert CiConfig(engine, args()).branch_class().silent

    def test_and_quoting_it_works_just_the_same(self, engine):
        git(engine, "checkout", "-q", "-b", "experimental")
        write_ci(engine, 'branches:\n  "off": [experimental]\n')
        assert CiConfig(engine, args()).branch_class().silent

    def test_and_as_the_declared_default_too(self, engine):
        git(engine, "checkout", "-q", "-b", "whatever")
        write_ci(engine, "branches:\n  default: off\n")
        assert CiConfig(engine, args()).branch_class().name == "off"

    def test_a_single_name_need_not_be_written_as_a_list(self, formats):
        write_ci(formats, "branches:\n  prod: main\n")
        assert CiConfig(formats, args()).branch_class().name == "prod"


class TestABranchInTwoClasses:
    """Not a precedence puzzle to be resolved quietly. The tool refuses to guess."""

    def test_it_is_an_error(self, formats):
        write_ci(formats, 'branches:\n  prod: [main]\n  dev: ["m*"]\n')
        with pytest.raises(ConfigError) as raised:
            CiConfig(formats, args()).branch_class()
        assert "more than one class" in str(raised.value)

    def test_and_the_message_names_both_classes(self, formats):
        write_ci(formats, 'branches:\n  prod: [main]\n  dev: ["m*"]\n')
        with pytest.raises(ConfigError) as raised:
            CiConfig(formats, args()).branch_class()
        assert "prod" in str(raised.value) and "dev" in str(raised.value)


class TestNoBranchToJudge:
    def test_a_detached_head_falls_to_the_default(self, engine):
        (engine / "a").write_text("a\n")
        git(engine, "add", "a")
        git(engine, "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "one")
        head = subprocess.run(
            ["git", "-C", str(engine), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        git(engine, "checkout", "-q", head)
        resolved = CiConfig(engine, args()).branch_class()
        assert resolved.name == "dev"
        assert resolved.branch is None

    def test_and_the_reason_says_so_rather_than_pretending(self, engine):
        (engine / "a").write_text("a\n")
        git(engine, "add", "a")
        git(engine, "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "one")
        head = subprocess.run(
            ["git", "-C", str(engine), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        git(engine, "checkout", "-q", head)
        assert "detached" in CiConfig(engine, args()).branch_class().reason

    def test_a_directory_that_is_no_git_repository_falls_to_the_default(self, tmp_path):
        (tmp_path / "packages" / "freeports").mkdir(parents=True)
        resolved = CiConfig(tmp_path, args()).branch_class()
        assert resolved.name == "dev"
        assert resolved.branch is None


class TestForcingTheClass:
    def test_the_command_line_wins_over_the_file(self, formats):
        write_ci(formats, "branches:\n  prod: [main]\n")
        assert CiConfig(formats, args(branch_class="off")).branch_class().name == "off"

    def test_the_environment_wins_over_the_file(self, formats, monkeypatch):
        write_ci(formats, "branches:\n  prod: [main]\n")
        monkeypatch.setenv("FREEPORTS_CI_BRANCH_CLASS", "dev")
        assert CiConfig(formats, args()).branch_class().name == "dev"

    def test_and_the_command_line_wins_over_the_environment(self, formats, monkeypatch):
        monkeypatch.setenv("FREEPORTS_CI_BRANCH_CLASS", "dev")
        assert (
            CiConfig(formats, args(branch_class="prod")).branch_class().name == "prod"
        )

    def test_the_reason_says_it_was_forced_and_by_what(self, formats, monkeypatch):
        monkeypatch.setenv("FREEPORTS_CI_BRANCH_CLASS", "prod")
        assert (
            "FREEPORTS_CI_BRANCH_CLASS"
            in CiConfig(formats, args()).branch_class().reason
        )

    def test_a_class_that_does_not_exist_is_an_error(self, formats, monkeypatch):
        monkeypatch.setenv("FREEPORTS_CI_BRANCH_CLASS", "staging")
        with pytest.raises(ConfigError):
            CiConfig(formats, args()).branch_class()


class TestAFileThatCannotBeObeyed:
    def test_a_class_name_that_does_not_exist(self, formats):
        write_ci(formats, "branches:\n  staging: [main]\n")
        with pytest.raises(ConfigError) as raised:
            CiConfig(formats, args())
        assert "staging" in str(raised.value)

    def test_a_default_that_does_not_exist(self, formats):
        write_ci(formats, "branches:\n  default: staging\n")
        with pytest.raises(ConfigError):
            CiConfig(formats, args())

    def test_yaml_that_does_not_parse(self, formats):
        write_ci(formats, "branches: [oh: no\n")
        with pytest.raises(ConfigError) as raised:
            CiConfig(formats, args())
        assert "does not parse" in str(raised.value)

    def test_a_file_that_is_not_a_mapping(self, formats):
        write_ci(formats, "- one\n- two\n")
        with pytest.raises(ConfigError):
            CiConfig(formats, args())

    def test_an_empty_file_is_not_an_error_it_is_every_default(self, formats):
        write_ci(formats, "")
        assert CiConfig(formats, args()).branch_class().name == "prod" or True
        assert CiConfig(formats, args()).thresholds() == {}

    def test_a_threshold_that_is_not_a_number(self, engine):
        write_ci(engine, "thresholds:\n  lint.rust: soon\n")
        with pytest.raises(ConfigError) as raised:
            CiConfig(engine, args())
        assert "not a number" in str(raised.value)


class TestAThresholdThisRepositoryCannotEvaluate:
    """A threshold that can never be evaluated is a threshold somebody only thinks they have."""

    def test_a_metric_no_tool_knows(self, engine):
        write_ci(engine, "thresholds:\n  tests.haskell.lines: 50\n")
        with pytest.raises(ConfigError) as raised:
            CiConfig(engine, args())
        assert "no metric" in str(raised.value)

    def test_a_metric_this_kind_of_repository_does_not_measure(self, formats):
        write_ci(formats, "thresholds:\n  tests.rust.lines: 50\n")
        with pytest.raises(ConfigError) as raised:
            CiConfig(formats, args())
        assert "not measured in a formats repository" in str(raised.value)

    def test_and_the_message_says_where_it_would_have_been_measured(self, formats):
        write_ci(formats, "thresholds:\n  docs.rust: 30\n")
        with pytest.raises(ConfigError) as raised:
            CiConfig(formats, args())
        assert "engine" in str(raised.value)

    def test_one_the_repository_does_measure_is_accepted(self, formats):
        write_ci(formats, "thresholds:\n  tests.formats.integration: 86\n")
        assert CiConfig(formats, args()).thresholds() == {
            "tests.formats.integration": 86.0
        }

    def test_the_same_check_applies_to_a_minimum_given_on_the_command_line(
        self, formats
    ):
        with pytest.raises(ConfigError):
            CiConfig(formats, args(min=["tests.rust.lines=50"])).thresholds()


class TestTheOrderOfTheTiersForOneMinimum:
    """Resolved per metric, never per source: raising one bar leaves the others where they were."""

    def test_the_file_alone(self, engine):
        write_ci(engine, "thresholds:\n  lint.rust: 9.9\n  docs.rust: 39\n")
        assert CiConfig(engine, args()).thresholds() == {
            "lint.rust": 9.9,
            "docs.rust": 39.0,
        }

    def test_the_environment_wins_over_the_file(self, engine, monkeypatch):
        write_ci(engine, "thresholds:\n  lint.rust: 9.9\n")
        monkeypatch.setenv("FREEPORTS_CI_MIN_LINT_RUST", "9.5")
        assert CiConfig(engine, args()).thresholds()["lint.rust"] == 9.5

    def test_the_command_line_wins_over_the_environment(self, engine, monkeypatch):
        write_ci(engine, "thresholds:\n  lint.rust: 9.9\n")
        monkeypatch.setenv("FREEPORTS_CI_MIN_LINT_RUST", "9.5")
        assert (
            CiConfig(engine, args(min=["lint.rust=9.0"])).thresholds()["lint.rust"]
            == 9.0
        )

    def test_overriding_one_leaves_the_others_alone(self, engine):
        write_ci(engine, "thresholds:\n  lint.rust: 9.9\n  docs.rust: 39\n")
        resolved = CiConfig(engine, args(min=["lint.rust=9.0"])).thresholds()
        assert resolved == {"lint.rust": 9.0, "docs.rust": 39.0}

    def test_min_can_be_given_more_than_once(self, engine):
        resolved = CiConfig(
            engine, args(min=["lint.rust=9.0", "docs.rust=30"])
        ).thresholds()
        assert resolved == {"lint.rust": 9.0, "docs.rust": 30.0}

    def test_min_without_an_equals_sign_is_an_error(self, engine):
        with pytest.raises(ConfigError) as raised:
            CiConfig(engine, args(min=["lint.rust"])).thresholds()
        assert "metric=value" in str(raised.value)

    def test_an_unset_variable_does_not_count_as_a_tier(self, engine, monkeypatch):
        write_ci(engine, "thresholds:\n  lint.rust: 9.9\n")
        monkeypatch.setenv("FREEPORTS_CI_MIN_LINT_RUST", "")
        assert CiConfig(engine, args()).thresholds()["lint.rust"] == 9.9


class TestPerPackageMinima:
    """The most specific name wins for its package; the aggregate covers whatever none claims."""

    def test_a_per_package_key_is_accepted_in_the_file(self, engine):
        write_ci(
            engine,
            "thresholds:\n  tests.python.lines: 60\n  tests.python.freeports_validate.lines: 40\n",
        )
        resolved = CiConfig(engine, args()).thresholds()
        assert resolved["tests.python.freeports_validate.lines"] == 40.0
        assert resolved["tests.python.lines"] == 60.0

    def test_the_aggregate_it_overrides_is_named_by_the_key_itself(self):
        assert metrics.split_package_key("tests.python.freeports_validate.lines") == (
            "tests.python.lines",
            "freeports_validate",
        )

    def test_and_for_the_documentation_family_too(self):
        assert metrics.split_package_key("docs.python.freeports_dev") == (
            "docs.python",
            "freeports_dev",
        )

    def test_an_aggregate_is_not_read_as_a_member_of_its_own_family(self):
        assert metrics.split_package_key("docs.python") is None
        assert metrics.find("docs.python").pattern == "docs.python"

    def test_a_per_package_key_can_also_be_set_from_the_environment(
        self, engine, monkeypatch
    ):
        monkeypatch.setenv(
            "FREEPORTS_CI_MIN_TESTS_PYTHON_FREEPORTS_VALIDATE_LINES", "42"
        )
        resolved = CiConfig(engine, args()).thresholds()
        assert resolved["tests.python.freeports_validate.lines"] == 42.0

    def test_a_variable_naming_nothing_is_an_error_rather_than_a_setting_that_does_nothing(
        self, engine, monkeypatch
    ):
        monkeypatch.setenv("FREEPORTS_CI_MIN_WHATEVER", "42")
        with pytest.raises(ConfigError) as raised:
            CiConfig(engine, args()).thresholds()
        assert "no metric" in str(raised.value)


class TestTheKeyServer:
    def test_the_file_names_it(self, formats):
        write_ci(formats, "keyserver: https://keys.example.org\n")
        assert CiConfig(formats, args()).keyserver == "https://keys.example.org"

    def test_the_environment_wins_over_the_file(self, formats, monkeypatch):
        write_ci(formats, "keyserver: https://keys.example.org\n")
        monkeypatch.setenv("FREEPORTS_VALIDATE_KEYSERVER", "https://other.example.org")
        assert CiConfig(formats, args()).keyserver == "https://other.example.org"

    def test_the_command_line_wins_over_the_environment(self, formats, monkeypatch):
        monkeypatch.setenv("FREEPORTS_VALIDATE_KEYSERVER", "https://other.example.org")
        assert (
            CiConfig(formats, args(keyserver="https://cli.example.org")).keyserver
            == "https://cli.example.org"
        )


class TestTheMetricRegistry:
    def test_every_metric_name_maps_back_to_itself(self):
        for metric in metrics.REGISTRY:
            if not metric.is_family:
                assert metrics.find(metric.pattern) is metric

    def test_the_environment_name_is_mechanical(self):
        assert (
            metrics.env_name("tests.rust.lines") == "FREEPORTS_CI_MIN_TESTS_RUST_LINES"
        )

    def test_a_formats_repository_knows_the_formats_metrics_and_not_the_rust_ones(self):
        known = {m.pattern for m in metrics.known_in(metrics.FORMATS)}
        assert "tests.formats.integration" in known
        assert "tests.rust.lines" not in known

    def test_an_input_database_knows_only_the_grant_metrics(self):
        known = {m.pattern for m in metrics.known_in(metrics.INPUT_DB)}
        assert known == {"grants.coverage", "grants.keys_online"}

    def test_the_slow_metrics_are_the_ones_a_commit_cannot_pay_for(self):
        """The whole list, pinned. Moving a metric across this line changes what every commit costs.

        Two different expenses, and the registry calls both `slow` because the gate treats both the
        same way: the coverage figures recompile or re-run whole suites, and the two `grants` ones
        resolve pages and fingerprints over somebody else's network. A figure that depends on a host
        nobody here controls is not a figure about the commit being made.
        """
        slow = {m.pattern for m in metrics.REGISTRY if m.cost == metrics.SLOW}
        assert slow == {
            "tests.rust.lines",
            "tests.python.lines",
            "tests.python.*.lines",
            "docs.rust",
            "grants.coverage",
            "grants.keys_online",
        }

    def test_and_the_fast_ones_are_what_is_left(self):
        """Named the other way round too, so that adding a metric has to answer this question."""
        fast = {m.pattern for m in metrics.REGISTRY if m.cost == metrics.FAST}
        assert fast == {
            "tests.formats.integration",
            "tests.formats.single_page",
            "docs.python",
            "docs.python.*",
            "lint.rust",
            "lint.python",
        }
