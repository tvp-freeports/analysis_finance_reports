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
