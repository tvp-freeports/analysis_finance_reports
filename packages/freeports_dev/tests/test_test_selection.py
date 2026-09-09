"""Which tests `freeports-dev test` runs, and why the answer has to be the Makefile's answer.

A formats repository's suite has two halves — the per-page tests, which are the loop format
development runs in, and the whole-document tests, each a full extraction run — and there are two
ways to ask for either: a Makefile target and this command. **They have to mean the same thing.**
The Makefile is how you work on your own repository and this command is how you interrogate one that
is not yours, so a person will meet both, and a selection that meant one thing under `make test` and
another under `freeports-dev test` would be a trap laid in the one place the two surfaces touch.

The properties below are that seam: one spelling of each marker, in the command that owns it; names
that match the targets; and a selection of your own left to stand alone.
"""

import sys

import pytest

from freeports_dev import cli
from freeports_dev.ci import metrics, suites
from freeports_dev.repo_init import init_format_repo


@pytest.fixture(scope="module")
def repo(tmp_path_factory):
    """One initialised repository, built once: nothing below writes to it."""
    target = tmp_path_factory.mktemp("workspace") / "acme-formats"
    init_format_repo(target, quiet=True)
    return target


@pytest.fixture
def pytest_arguments(monkeypatch):
    """Capture what would be handed to pytest, and run nothing.

    The question here is *which tests were selected*, and answering it by running them would make
    every case cost a session and depend on there being formats in the repository at all.
    """
    captured = []

    def fake_main(arguments):
        captured.append(list(arguments))
        return 0

    monkeypatch.setattr(pytest, "main", fake_main)
    return captured


def run(monkeypatch, repo, *arguments):
    """`freeports-dev test --repo <repo> …`, through the real parser, catching its exit."""
    monkeypatch.setattr(
        sys, "argv", ["freeports-dev", "test", "--repo", str(repo), *arguments]
    )
    with pytest.raises(SystemExit) as exit:
        cli.main()
    return exit.value.code


class TestTheTwoHalves:
    def test_there_are_three_selections_and_no_more(self):
        assert set(cli.TEST_SELECTIONS) == {"fast", "slow", "all"}

    def test_each_half_names_a_suite_the_registry_knows(self):
        """A name nothing has a policy for is reported under a name the gate has never heard of."""
        known = {suite.name for suite in suites.known_in(metrics.FORMATS)}
        named = {
            suite
            for _, suite, _, _ in cli.TEST_SELECTIONS.values()
            if suite is not None
        }
        assert named == known

    def test_the_registry_points_at_the_makefile_target_that_runs_each(self):
        """A verdict that says a suite is stale without saying how to un-stale it is one people
        route around — and the command it quotes has to be one that exists."""
        for suite in suites.known_in(metrics.FORMATS):
            assert suite.command in ("make test-fast", "make test-slow")

    def test_both_halves_together_are_the_whole_suite(self):
        """`--all` selects by not selecting: a marker of its own would be a third thing to keep in
        step with the other two."""
        assert cli.TEST_SELECTIONS["all"][0] is None


class TestWhatEachFlagSelects:
    def test_the_default_is_the_fast_half(self, monkeypatch, repo, pytest_arguments):
        """The same default `make test` has: `test` is an alias of `test-fast` in both places."""
        run(monkeypatch, repo)
        assert pytest_arguments[0][-2:] == ["-m", "not integration_tests"]

    def test_fast_is_the_per_page_tests(self, monkeypatch, repo, pytest_arguments):
        run(monkeypatch, repo, "--fast")
        assert pytest_arguments[0][-2:] == ["-m", "not integration_tests"]

    def test_slow_is_the_whole_document_tests(
        self, monkeypatch, repo, pytest_arguments
    ):
        run(monkeypatch, repo, "--slow")
        assert pytest_arguments[0][-2:] == ["-m", "integration_tests"]

    def test_all_deselects_nothing(self, monkeypatch, repo, pytest_arguments):
        run(monkeypatch, repo, "--all")
        assert "-m" not in pytest_arguments[0]

    def test_they_are_three_ways_of_answering_one_question(self, monkeypatch, repo):
        """Mutually exclusive, so `--fast --slow` is refused rather than silently meaning one."""
        monkeypatch.setattr(
            sys,
            "argv",
            ["freeports-dev", "test", "--repo", str(repo), "--fast", "--slow"],
        )
        with pytest.raises(SystemExit) as exit:
            cli.main()
        assert exit.value.code == 2


class TestASelectionOfYourOwn:
    """`-- -m …` outranks the flag, and is left to stand alone.

    pytest takes the *last* `-m` on the command line, so one added beside yours would not even be an
    error: it would quietly replace the selection you asked for. Selections this command has no
    vocabulary for — `not integration_tests and not xfail`, a single test's marker — have to stay
    possible, and this is what keeps them so.
    """

    def test_your_marker_is_the_only_one(self, monkeypatch, repo, pytest_arguments):
        run(monkeypatch, repo, "--", "-m", "not integration_tests and not xfail")
        assert pytest_arguments[0].count("-m") == 1
        assert "not integration_tests and not xfail" in pytest_arguments[0]

    def test_even_when_a_flag_asked_for_the_other_half(
        self, monkeypatch, repo, pytest_arguments
    ):
        run(monkeypatch, repo, "--slow", "--", "-m", "some_other_marker")
        assert pytest_arguments[0].count("-m") == 1
        assert "integration_tests" not in pytest_arguments[0]

    def test_the_attached_spelling_counts_too(
        self, monkeypatch, repo, pytest_arguments
    ):
        """`-mfoo` is the same flag as `-m foo`, and reading only the detached form would add a
        second `-m` to a command line that already had one."""
        run(monkeypatch, repo, "--", "-mfoo")
        assert not any(argument == "-m" for argument in pytest_arguments[0])

    def test_other_pytest_arguments_do_not_count_as_a_marker(
        self, monkeypatch, repo, pytest_arguments
    ):
        """`-x` says nothing about which tests were asked for, so the selection still applies."""
        run(monkeypatch, repo, "--", "-x")
        assert pytest_arguments[0][-3:] == ["-m", "not integration_tests", "-x"]


class TestASuiteWithNothingInItHasNotFailed:
    """pytest exits 5 when it collects nothing, and that is not a failure here.

    A repository `init-format-repo` has just created has no formats in it yet, and a suite with no
    tests in it has passed vacuously, the way an empty directory of tests passes. Read as a failure
    it makes every freshly bootstrapped repository report a broken suite — at exactly the moment a
    new user is deciding whether to trust any of this.
    """

    def test_nothing_collected_is_not_a_failure(self, monkeypatch, repo):
        monkeypatch.setattr(pytest, "main", lambda arguments: cli.NOTHING_COLLECTED)
        assert run(monkeypatch, repo) == 0

    def test_a_real_failure_still_is_one(self, monkeypatch, repo):
        monkeypatch.setattr(pytest, "main", lambda arguments: 1)
        assert run(monkeypatch, repo) == 1

    def test_and_a_passing_run_passes(self, monkeypatch, repo):
        monkeypatch.setattr(pytest, "main", lambda arguments: 0)
        assert run(monkeypatch, repo) == 0
