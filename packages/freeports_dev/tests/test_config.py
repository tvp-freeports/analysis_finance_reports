"""Which tier a `freeports-dev` setting comes from, and — for a path — what it is relative to.

The order is the engine's own: command line, then environment, then configuration file, then the
tool's default. That part is uncontroversial. What is not is the question a path raises on top of
it: *relative to where?*

**Each tier is resolved against the place it was written, and the three places differ.** A path typed
as `--repo`, or exported into the environment, is relative to the shell you are standing in — that is
the only thing it can mean. A path in a configuration file is not: the file is *searched for*, so one
line in it is read from many different working directories and has to name the same repository from
all of them. Relative to the file that says it, it does.

Resolving a file's path against the working directory instead is not a theoretical problem. It is
how `formats_repo: analysis_finance_reports_formats`, in the workspace file one level above that
repository, became `analysis_finance_reports_formats/analysis_finance_reports_formats` the moment
anybody ran `freeports-dev test` from inside it — which is exactly where a `pre-commit` hook runs.
`freeports-validate` fixed this for itself and wrote the reason down; these tests are that same rule
held for `freeports-dev`.
"""

from argparse import Namespace
from pathlib import Path

import pytest

from freeports_dev.config import DevConfig


pytest.importorskip(
    "freeports.cli", reason="reading a configuration file needs the engine"
)


def args(**overrides):
    """A parsed command line with nothing on it but what a test names."""
    return Namespace(
        **{"config": None, "repo": None, "db_directory": None, **overrides}
    )


@pytest.fixture
def workspace(tmp_path):
    """A configuration file one level above the repository it names, which is the shape that broke.

    `formats_repo` is written the way a person writes it — the name of a sibling directory, relative
    to the file — and the repository itself exists, so a test can also stand *inside* it.
    """
    (tmp_path / "my-formats" / "metadata").mkdir(parents=True)
    (tmp_path / "my-formats" / "metadata" / "formats.csv").write_text(
        "Name,Locale,Year\n"
    )
    (tmp_path / "databases" / "real").mkdir(parents=True)
    config = tmp_path / "freeports-conf.yaml"
    config.write_text("formats_repo: my-formats\ndb_path: databases/real\n")
    return tmp_path


class TestAPathTheConfigurationFileGave:
    def test_it_is_relative_to_the_file_not_to_the_working_directory(
        self, workspace, monkeypatch, tmp_path
    ):
        monkeypatch.chdir(tmp_path.parent)
        resolved = DevConfig(
            args(config=str(workspace / "freeports-conf.yaml"))
        ).formats_repo
        assert resolved == workspace / "my-formats"

    def test_and_it_stays_the_same_from_inside_the_repository_it_names(
        self, workspace, monkeypatch
    ):
        """The regression: this is where a `pre-commit` hook runs, and it doubled the last segment."""
        monkeypatch.chdir(workspace / "my-formats")
        resolved = DevConfig(
            args(config=str(workspace / "freeports-conf.yaml"))
        ).formats_repo
        assert resolved == workspace / "my-formats"
        assert resolved != workspace / "my-formats" / "my-formats"

    def test_and_from_anywhere_else_under_it(self, workspace, monkeypatch):
        deep = workspace / "my-formats" / "metadata"
        monkeypatch.chdir(deep)
        assert DevConfig(
            args(config=str(workspace / "freeports-conf.yaml"))
        ).formats_repo == (workspace / "my-formats")

    def test_an_absolute_path_is_left_alone(self, tmp_path, monkeypatch):
        elsewhere = tmp_path / "elsewhere"
        elsewhere.mkdir()
        config = tmp_path / "freeports-conf.yaml"
        config.write_text(f"formats_repo: {elsewhere}\n")
        monkeypatch.chdir(tmp_path)
        assert DevConfig(args(config=str(config))).formats_repo == elsewhere

    def test_the_database_path_follows_the_same_rule(self, workspace, monkeypatch):
        monkeypatch.chdir(workspace / "my-formats")
        resolved = DevConfig(
            args(config=str(workspace / "freeports-conf.yaml"))
        ).input_db_from_file
        assert Path(resolved) == workspace / "databases" / "real"

    def test_a_file_found_by_searching_gets_the_same_treatment(
        self, workspace, monkeypatch
    ):
        """`--config` is not what a hook uses; the searched file is, so it must behave the same."""
        monkeypatch.chdir(workspace / "my-formats")
        monkeypatch.setenv(
            "FREEPORTS_CONFIG_FILE", str(workspace / "freeports-conf.yaml")
        )
        assert DevConfig(args()).formats_repo == workspace / "my-formats"


class TestAPathGivenOnTheCommandLineOrInTheEnvironment:
    """Those two are relative to the shell you typed them in, and must stay that way."""

    def test_the_command_line_is_relative_to_the_working_directory(
        self, workspace, monkeypatch
    ):
        monkeypatch.chdir(workspace)
        assert (
            DevConfig(args(repo="my-formats")).formats_repo == workspace / "my-formats"
        )

    def test_and_it_is_not_quietly_moved_next_to_the_configuration_file(
        self, workspace, monkeypatch, tmp_path
    ):
        inner = workspace / "my-formats"
        (inner / "my-formats").mkdir()
        monkeypatch.chdir(inner)
        resolved = DevConfig(
            args(repo="my-formats", config=str(workspace / "freeports-conf.yaml"))
        ).formats_repo
        assert resolved == inner / "my-formats"

    def test_the_environment_is_relative_to_the_working_directory(
        self, workspace, monkeypatch
    ):
        monkeypatch.chdir(workspace)
        monkeypatch.setenv("FREEPORTS_FORMATS_REPO_PATH", "my-formats")
        assert DevConfig(args()).formats_repo == workspace / "my-formats"


class TestTheOrderOfTheTiers:
    def test_the_command_line_wins_over_the_environment(self, workspace, monkeypatch):
        monkeypatch.chdir(workspace)
        monkeypatch.setenv("FREEPORTS_FORMATS_REPO_PATH", str(workspace / "databases"))
        assert (
            DevConfig(args(repo="my-formats")).formats_repo == workspace / "my-formats"
        )

    def test_the_environment_wins_over_the_file(self, workspace, monkeypatch):
        monkeypatch.chdir(workspace)
        monkeypatch.setenv("FREEPORTS_FORMATS_REPO_PATH", "databases")
        resolved = DevConfig(
            args(config=str(workspace / "freeports-conf.yaml"))
        ).formats_repo
        assert resolved == workspace / "databases"

    def test_with_nothing_said_anywhere_it_is_the_working_directory(
        self, tmp_path, monkeypatch
    ):
        monkeypatch.chdir(tmp_path)
        monkeypatch.delenv("FREEPORTS_CONFIG_FILE", raising=False)
        monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path / "no-config"))
        assert DevConfig(args()).formats_repo == Path.cwd()
