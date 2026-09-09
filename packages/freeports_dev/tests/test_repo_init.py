"""What `freeports-dev init-format-repo` leaves behind, and why each piece has to be there.

A format repository is bootstrapped once and lived in for years, so what the skeleton contains is
what most repositories will still contain: whatever is missing here has to be invented separately by
every person who ever makes one. These tests are about the half of it that carries the validation
report — the README, the six pages that host the lookup tables, the badges, and the hook that keeps
all of them current.

Two properties matter more than the file list:

**Every generated file is marked.** `freeports-validate report` rewrites what is between two
markers and refuses to guess where they go, so a page created without them is a page the hook can
never fill. Creating them here is what makes the hook a refresh rather than a first draft.

**Nothing about the report can stop the repository being used.** The figures depend on resolving
methodology pages over a network, and neither `init-format-repo` nor the hook may fail because that
network is not there.
"""

import json
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

from freeports_dev.repo_init import init_format_repo


LIB = Path(sys.modules["freeports_dev"].__file__).parent / "lib"

#: The six arrangements the report offers, read from the same data file the code reads, so a
#: seventh added there is covered here without anybody remembering to add it.
TABLES = [
    entry["table"] for entry in json.loads((LIB / "report_tables.json").read_text())
]

MARKERS = ("<!-- freeports-validate:begin -->", "<!-- freeports-validate:end -->")

#: The arrangements the CI report offers, read from the data file the code reads for the same
#: reason as above: a third added there is covered here without anybody remembering to.
CI_TABLES = [
    entry["table"] for entry in json.loads((LIB / "ci_report_tables.json").read_text())
]

CI_MARKERS = ("<!-- freeports-dev:begin -->", "<!-- freeports-dev:end -->")


@pytest.fixture(scope="module")
def repo(tmp_path_factory):
    """One initialised repository, built once: `init-format-repo` copies an input database.

    Module-scoped because nothing below writes to it, and because the copy is the slowest thing
    here by a wide margin — a fresh one per test would pay for it a dozen times over for no
    property any of them asserts.
    """
    target = tmp_path_factory.mktemp("workspace") / "acme-formats"
    init_format_repo(target, quiet=True)
    return target


class TestTheFilesThatHostTheReport:
    def test_there_is_a_readme_where_a_reader_looks_first(self, repo):
        assert (repo / "README.md").is_file()

    def test_the_readme_is_named_after_the_repository(self, repo):
        assert (repo / "README.md").read_text().startswith("# acme-formats\n")

    def test_there_is_one_page_per_arrangement_of_the_grants(self, repo):
        for table in TABLES:
            assert (repo / "validation" / "report" / f"{table}.md").is_file()

    def test_there_are_six_of_them(self, repo):
        """Three dimensions taken two at a time: the three lookups, each with and without its flag."""
        pages = sorted((repo / "validation" / "report").glob("*.md"))
        assert len(pages) == 6

    def test_each_page_is_named_after_the_table_that_fills_it(self, repo):
        """So the line in the hook and the file it writes read the same."""
        pages = {page.stem for page in (repo / "validation" / "report").glob("*.md")}
        assert pages == set(TABLES)

    def test_the_badges_have_somewhere_to_go(self, repo):
        assert (repo / "validation" / "report" / "badges").is_dir()


class TestEveryGeneratedFileIsMarked:
    """A page without the marker pair is a page `report` refuses to write, for ever."""

    def test_the_readme_says_where_the_summary_goes(self, repo):
        text = (repo / "README.md").read_text()
        for marker in MARKERS:
            assert marker in text

    @pytest.mark.parametrize("table", TABLES)
    def test_every_page_says_where_its_table_goes(self, repo, table):
        text = (repo / "validation" / "report" / f"{table}.md").read_text()
        for marker in MARKERS:
            assert marker in text

    def test_the_begin_marker_comes_first(self, repo):
        text = (repo / "README.md").read_text()
        assert text.index(MARKERS[0]) < text.index(MARKERS[1])


class TestThePagesExplainThemselves:
    @pytest.mark.parametrize("table", TABLES)
    def test_a_page_names_the_lookup_it_is_the_written_down_form_of(self, repo, table):
        entries = json.loads((LIB / "report_tables.json").read_text())
        mirrors = next(e["mirrors"] for e in entries if e["table"] == table)
        assert mirrors in (repo / "validation" / "report" / f"{table}.md").read_text()

    @pytest.mark.parametrize("table", TABLES)
    def test_a_page_says_how_to_rewrite_it_by_hand(self, repo, table):
        text = (repo / "validation" / "report" / f"{table}.md").read_text()
        assert f"--table {table}" in text

    @pytest.mark.parametrize("table", TABLES)
    def test_a_page_says_that_what_is_between_the_markers_is_replaced(
        self, repo, table
    ):
        """Nobody should learn that by having their own paragraph deleted."""
        text = (repo / "validation" / "report" / f"{table}.md").read_text()
        assert "generated" in text

    def test_the_readme_links_every_page(self, repo):
        text = (repo / "README.md").read_text()
        for table in TABLES:
            assert f"validation/report/{table}.md" in text

    def test_the_readme_shows_the_three_badges(self, repo):
        text = (repo / "README.md").read_text()
        for badge in ("grants-total", "grants-coverage", "check-grants"):
            assert f"validation/report/badges/{badge}.svg" in text


class TestTheHook:
    @pytest.fixture
    def hook(self, repo):
        return (repo / ".githooks" / "pre-commit").read_text()

    def test_it_is_executable(self, repo):
        assert (repo / ".githooks" / "pre-commit").stat().st_mode & 0o111

    def test_it_is_a_valid_shell_script(self, repo):
        """A hook with a syntax error fails every commit, and says nothing useful about why."""
        checked = subprocess.run(
            ["sh", "-n", str(repo / ".githooks" / "pre-commit")], capture_output=True
        )
        assert checked.returncode == 0, checked.stderr

    def test_it_walks_the_repository_once(self, hook):
        """Nine artefacts out of nine collections would resolve every page nine times."""
        assert hook.count("collect >") == 1
        assert "--model" in hook

    def test_it_refreshes_the_badges_the_readme_and_every_page(self, hook):
        assert "--format badges" in hook
        assert "README.md" in hook


class TestWhichHalfOfTheHookRunsOnWhichBranch:
    """The fast/slow split, as the generated hook implements it.

    These read the script as text, which is the right level for exactly this file: it is a shell
    program shipped as a data file, and what has to hold about it are properties of its source —
    that a name matches the registry, that a branch guard is present. Running it end to end is a
    different test with a different cost, and it would need a repository with formats in it, a
    keyring and a network.
    """

    @pytest.fixture
    def hook(self, repo):
        return (repo / ".githooks" / "pre-commit").read_text()

    def test_the_names_it_records_are_the_ones_the_registry_knows(self, hook):
        """A typo here would create a suite nothing has a policy for, reported under a name the
        gate has never heard of — which is the one failure a registry exists to prevent."""
        from freeports_dev.ci import metrics, suites

        for suite in suites.known_in(metrics.FORMATS):
            assert f'--suite "{suite.name}:' in hook

    def test_the_per_page_suite_runs_unconditionally(self, hook):
        assert '-m "not integration_tests"' in hook

    def test_the_whole_document_suite_runs_only_on_a_prod_branch(self, hook):
        """Ninety-five seconds of full extraction runs, which is not a per-commit cost."""
        before, _, after = hook.partition('-m "integration_tests"')
        assert (
            'if [ "$branch_class" = "prod" ]; then'
            in before.split('-m "not integration_tests"')[-1]
        )
        assert after

    def test_nothing_that_reaches_the_network_runs_on_a_dev_branch(self, hook):
        """A commit has to be possible on a train, and six seconds of somebody else's server is
        the single most expensive thing this hook could do at every commit."""
        for command in ("collect >", "check-grants", "check-keys"):
            index = hook.index(command)
            assert 'if [ "$branch_class" = "prod" ]; then' in hook[:index]

    def test_an_empty_suite_is_passed_rather_than_failed(self, hook):
        """pytest exits 5 when it collects nothing, and a repository this command has just created
        has no formats yet. Read as a failure it would make every freshly bootstrapped repository
        report a broken suite — at the moment a new user decides whether to trust the hook."""
        assert hook.count("0|5)") == 2

    def test_the_report_is_not_rewritten_from_a_walk_that_resolved_nothing(self, hook):
        """Otherwise a network outage is committed as `coverage --, check-grants failing`, which is
        a claim about somebody's web server written into tracked files."""
        assert '"state": *"unmeasured"' in hook
        assert "no methodology page could be resolved" in hook

    def test_every_rendering_is_asked_for_in_one_invocation(self, hook):
        """Five invocations are five interpreter starts, and five evaluations that could disagree
        with one another about the same run."""
        assert hook.count("freeports-dev ci-report") == 1
        assert "--render badges:" in hook

    def test_the_verdict_makes_no_claim_of_its_own_about_a_suite(self, hook):
        """`ci-check --suite ...` was a claim typed on a command line about a run that may never
        have happened, and silent about every suite that did not."""
        assert "freeports-dev ci-check --repo" in hook
        assert 'ci-check --repo "$repo_root" --suite' not in hook
        for table in TABLES:
            assert table in hook

    def test_it_names_the_repository_git_is_committing(self, hook):
        """A hook has no business asking where it is: git already knows."""
        assert "git rev-parse --show-toplevel" in hook

    def test_and_hands_that_to_both_commands(self, hook):
        """Left to resolve it themselves, either can be pointed elsewhere by a stray tier.

        A relative `FREEPORTS_FORMATS_REPO_PATH` carried in from another directory, or a
        `formats_repo:` line in a configuration file found from here, both name a different
        repository than the one being committed — and the failure reads as the repository not
        being a repository, with its last path segment doubled.
        """
        for command in ("freeports-dev test", "freeports-validate "):
            for line in hook.splitlines():
                # Comments name the commands to explain them, `command -v` only asks whether one
                # exists, and `echo` says what happened -- none of the three is an invocation.
                stripped = line.lstrip()
                if stripped.startswith(("#", "echo ")) or "command -v" in line:
                    continue
                if command in line:
                    assert "--repo" in line or "$repo_root" in line, line

    def test_it_stages_what_it_rewrote(self, hook):
        """Otherwise the refresh is a dirty working tree rather than part of the commit."""
        assert "git add" in hook

    def test_it_does_nothing_when_the_command_is_not_installed(self, hook):
        assert "command -v freeports-validate" in hook

    def test_it_checks_the_grants(self, hook):
        """Rendering what `validation/` claims is not the same as checking that it still holds."""
        assert "check-grants" in hook

    def test_it_never_refuses_a_commit_over_the_report_or_the_grants(self, hook):
        """A committer on a train still has to be able to commit, and so does one without the key.

        The slice covers the grant check as well as the report: a granted output regenerated by
        `make-tests` is an ordinary step of writing a format, and a contributor who does not hold
        the key that granted a file has no way to clear such a gate at all.
        """
        block = hook[hook.index("command -v freeports-validate") :]
        block = block[: block.index("git rev-parse --verify HEAD")]
        assert "check-grants" in block
        assert "exit 1" not in block


class TestTheFirstFill:
    """`init-format-repo` runs the command once, so nothing it created starts out broken."""

    def test_the_badges_exist_rather_than_being_broken_images(self, repo):
        badges = repo / "validation" / "report" / "badges"
        for badge in ("grants-total", "grants-coverage", "check-grants"):
            assert (badges / f"{badge}.svg").is_file()
            assert (badges / f"{badge}.json").is_file()

    def test_the_readme_block_was_filled(self, repo):
        text = (repo / "README.md").read_text()
        between = text[text.index(MARKERS[0]) : text.index(MARKERS[1])]
        assert "0 grants by 0 contributors" in between

    @pytest.mark.parametrize("table", TABLES)
    def test_every_page_was_filled(self, repo, table):
        text = (repo / "validation" / "report" / f"{table}.md").read_text()
        between = text[text.index(MARKERS[0]) : text.index(MARKERS[1])]
        assert "check-grants:" in between

    def test_a_filled_page_says_what_it_is_worth(self, repo):
        """Every generated table disclaims itself; one that did not is the misreading to prevent."""
        text = (repo / "validation" / "report" / "methodology-file.md").read_text()
        assert "demonstrative" in text


class TestTheFilesThatHostTheCiReport:
    """The other half of what a repository publishes about itself: what a commit has to clear.

    The same shape as the validation report and deliberately so — a reader who has found one has
    found the other — with one difference that matters: these figures are measured locally, so the
    first fill of them needs no network at all.
    """

    @pytest.mark.parametrize("table", CI_TABLES)
    def test_every_page_exists_and_is_marked(self, repo, table):
        text = (repo / "ci" / "report" / f"{table}.md").read_text()
        assert CI_MARKERS[0] in text and CI_MARKERS[1] in text

    def test_the_readme_carries_the_summary_between_its_own_markers(self, repo):
        """Its own markers: the README holds this block and the grants one, side by side."""
        text = (repo / "README.md").read_text()
        assert CI_MARKERS[0] in text and MARKERS[0] in text

    def test_a_page_says_what_command_rewrites_it(self, repo):
        text = (repo / "ci" / "report" / "thresholds.md").read_text()
        assert "freeports-dev ci-report" in text

    def test_the_badges_exist_rather_than_being_broken_images(self, repo):
        badges = repo / "ci" / "report" / "badges"
        assert (badges / "ci-status.svg").is_file()
        assert (badges / "ci-status.json").is_file()

    def test_a_repository_that_has_measured_nothing_says_so_rather_than_claiming_a_pass(
        self, repo
    ):
        """The honest first state, and a more useful one than an empty file."""
        text = (repo / "README.md").read_text()
        between = text[text.index(CI_MARKERS[0]) : text.index(CI_MARKERS[1])]
        assert "inconclusive" in between
        assert "passing" not in between

    def test_every_page_disclaims_itself(self, repo):
        for table in CI_TABLES:
            assert (
                "demonstrative" in (repo / "ci" / "report" / f"{table}.md").read_text()
            )

    def test_the_hook_refreshes_it_and_cannot_refuse_over_it(self, repo):
        hook = (repo / ".githooks" / "pre-commit").read_text()
        block = hook[hook.index("Keep the CI report current") :]
        block = block[: block.index("The fingerprint rule")]
        assert "ci-report" in block
        assert "git -C" in block
        assert "exit 1" not in block


@pytest.fixture
def without_the_validate_command(monkeypatch):
    """A `PATH` that still has `git` on it but no `freeports-validate`.

    Emptying `PATH` outright would test something else -- a machine with no tools at all -- and the
    property under test is the narrow one: a repository whose *report* cannot be filled is still a
    repository. `git` stays because `init-format-repo` initialises one, and its absence is a
    different question from this one.
    """
    git = shutil.which("git")
    if git is None:
        pytest.skip(
            "git is not installed, so there is no PATH that has it and not the other"
        )
    directory = str(Path(git).parent)
    if shutil.which("freeports-validate", path=directory):
        pytest.skip("both commands are installed in the same directory")
    monkeypatch.setenv("PATH", directory)


class TestTheReportCannotStopTheRepositoryBeingCreated:
    def test_a_repository_is_created_even_with_no_validate_command(
        self, tmp_path, without_the_validate_command
    ):
        """The skeleton is the deliverable; the report is a convenience on top of it."""
        target = tmp_path / "no-tools"
        init_format_repo(target, quiet=True)
        assert (target / "README.md").is_file()
        assert (target / "package.yaml").is_file()
        for table in TABLES:
            assert (target / "validation" / "report" / f"{table}.md").is_file()

    def test_and_the_markers_are_still_there_to_fill_later(
        self, tmp_path, without_the_validate_command
    ):
        target = tmp_path / "no-tools-either"
        init_format_repo(target, quiet=True)
        assert MARKERS[0] in (target / "README.md").read_text()

    def test_it_says_which_command_to_run_once_the_tooling_is_there(
        self, tmp_path, capsys, without_the_validate_command
    ):
        """A note nobody can act on is the same as no note."""
        init_format_repo(tmp_path / "no-tools-third", quiet=True)
        assert "freeports-validate" in capsys.readouterr().out


class TestWhatWasAlreadyThere:
    """The skeleton this work extended, so extending it again does not quietly drop a piece."""

    def test_the_manifest_and_the_project_file(self, repo):
        assert (repo / "package.yaml").is_file()
        assert (repo / "pyproject.toml").is_file()

    def test_the_test_suite_scaffolding(self, repo):
        assert (repo / "tests" / "conftest.py").is_file()
        assert (repo / "tests" / "formats").is_dir()

    def test_the_input_database(self, repo):
        assert (repo / "tests" / "input_db").is_dir()

    def test_a_non_empty_target_is_refused(self, tmp_path):
        occupied = tmp_path / "occupied"
        occupied.mkdir()
        (occupied / "something.txt").write_text("mine")
        with pytest.raises(SystemExit):
            init_format_repo(occupied, quiet=True)


@pytest.fixture(scope="module")
def database(tmp_path_factory):
    """One initialised input database, built once — nothing below writes to it."""
    from freeports_dev.repo_init import init_input_db

    target = tmp_path_factory.mktemp("workspace") / "acme-db"
    init_input_db(target, quiet=True)
    return target


class TestAnInputDatabaseNowGetsACommitHook:
    """It used to get none, and the reasoning was sound as far as it went.

    A database has no test suite, so there was nothing for a hook to run. What that missed is that
    `metadata.yaml` declares a fingerprint of `companies/` and of `lists/`, and when the contents
    move the version has to move with them or the manifest holds a false statement about itself.
    Nothing computed those fingerprints, so nothing noticed. That is what the hook is for, and it
    is all it does.
    """

    @pytest.fixture
    def hook(self, database):
        return (database / ".githooks" / "pre-commit").read_text()

    def test_the_hook_exists(self, database):
        assert (database / ".githooks" / "pre-commit").is_file()

    def test_it_is_executable(self, database):
        assert (database / ".githooks" / "pre-commit").stat().st_mode & 0o111

    def test_it_is_a_valid_shell_script(self, database):
        """A hook with a syntax error fails every commit and says nothing useful about why."""
        checked = subprocess.run(
            ["sh", "-n", str(database / ".githooks" / "pre-commit")],
            capture_output=True,
        )
        assert checked.returncode == 0, checked.stderr

    def test_git_is_pointed_at_it(self, database):
        configured = subprocess.run(
            ["git", "-C", str(database), "config", "--local", "core.hooksPath"],
            capture_output=True,
            text=True,
        )
        assert configured.stdout.strip() == ".githooks"

    def test_it_checks_the_fingerprint_rule_and_nothing_else(self, hook):
        """Asserted over what the hook *runs*, not over its prose.

        The essay at the top says the words "no linted source" and "no test suite" precisely
        because those are the things it does not do, so matching the whole file would fail on the
        sentence explaining why it passes.
        """
        commands = "\n".join(
            line
            for line in hook.splitlines()
            if line.strip() and not line.lstrip().startswith("#")
        )
        assert "freeports-dev fingerprint" in commands
        assert "pytest" not in commands
        assert "lint-score" not in commands
        assert "coverage" not in commands

    def test_it_does_not_demand_an_environment(self, hook):
        """Computing sha256 sums needs no virtualenv, and refusing a CSV edit over one is a
        nuisance with nothing behind it."""
        assert "VIRTUAL_ENV" not in hook

    def test_without_freeports_dev_it_states_the_rule_and_lets_the_commit_through(
        self, hook
    ):
        """A database is edited by people who may have no Python environment at all."""
        assert "not on the PATH" in hook
        assert "exit 0" in hook

    def test_it_says_which_bump_each_directory_implies(self, hook):
        assert "minor" in hook and "major" in hook

    def test_it_prints_the_recipe_a_person_can_retype(self, hook):
        """A refusal nobody can check by hand is a refusal nobody believes."""
        assert "sha256sum" in hook
        assert "LC_ALL=C sort" in hook

    def test_an_off_branch_is_honoured_here_too(self, hook):
        assert "branch-class" in hook


class TestEveryRepositoryKindIsGatedByTheSameFile:
    """One name and one syntax in all three kinds, so the roles of the branches read the same."""

    def test_a_formats_repository_gets_a_ci_yaml(self, repo):
        assert (repo / "ci.yaml").is_file()

    def test_an_input_database_gets_the_same_one(self, database):
        assert (database / "ci.yaml").is_file()

    def test_it_parses(self, database):
        import yaml

        assert isinstance(yaml.safe_load((database / "ci.yaml").read_text()), dict)

    def test_it_declares_the_three_classes_and_a_default(self, database):
        import yaml

        branches = yaml.safe_load((database / "ci.yaml").read_text())["branches"]
        # `off` arrives as the boolean False: it is one of the ten words YAML 1.1 spells a boolean
        # with. The reader maps it back rather than making every repository quote it.
        assert {"prod", "dev", False, "default"} <= set(branches)
        assert branches["default"] == "dev"

    def test_a_branch_nobody_classified_is_dev_and_never_off(self, database):
        """The quiet answer must never be the default one."""
        import yaml

        assert (
            yaml.safe_load((database / "ci.yaml").read_text())["branches"]["default"]
            == "dev"
        )

    def test_it_seeds_no_threshold_at_all(self, database):
        """A minimum belongs at a figure somebody measured, and nobody has measured this yet."""
        import yaml

        assert yaml.safe_load((database / "ci.yaml").read_text())["thresholds"] == {}

    def test_the_gate_can_actually_read_what_was_written(self, database):
        """The template and the reader agreeing is the whole point of shipping the template."""
        from argparse import Namespace

        from freeports_dev.ci.config import CiConfig

        config = CiConfig(
            database,
            Namespace(repo=None, branch_class=None, min=None, keyserver=None),
        )
        assert config.branch_class().name in ("prod", "dev", "off")
        assert config.thresholds() == {}
