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
import re
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


class TestTheBuildSystem:
    """The `Makefile` is the repository's single entry point, and the hook only names a target.

    These read the file as text, which is the right level for it: it is a program shipped as a data
    file, and what has to hold about it are properties of its source — that a target exists, that a
    suite name matches the registry, that nothing which reaches the network is in the fast gate.
    Running the whole gate end to end is a different test with a different cost, and it would need a
    repository with formats in it, a keyring and a network.
    """

    @pytest.fixture
    def makefile(self, repo):
        return (repo / "Makefile").read_text()

    def test_there_is_a_makefile(self, repo):
        assert (repo / "Makefile").is_file()

    def test_there_is_a_windows_shim_beside_it(self, repo):
        """One build system, not one per platform: `make.bat` starts a POSIX shell and calls it."""
        assert (repo / "make.bat").is_file()
        assert "exec make" in (repo / "make.bat").read_text()

    def test_the_shim_holds_no_build_logic_of_its_own(self, repo):
        """Two formulations of the same build always diverge, so the second one holds nothing.

        It names a target in its own example line, which is documentation; what it must not have is
        a recipe, a path or a command of its own.
        """
        shim = [
            line
            for line in (repo / "make.bat").read_text().splitlines()
            if not line.lstrip().lower().startswith("rem")
        ]
        for logic in ("freeports-dev", "freeports-validate", "ruff", "pytest"):
            assert not any(logic in line for line in shim)

    def test_make_can_parse_it(self, repo):
        """A Makefile with a syntax error fails every commit and says nothing useful about why."""
        if shutil.which("make") is None:
            pytest.skip("make is not installed")
        parsed = subprocess.run(
            ["make", "-C", str(repo), "--dry-run", "help"], capture_output=True
        )
        assert parsed.returncode == 0, parsed.stderr

    @pytest.mark.parametrize(
        "target",
        [
            "help",
            "doctor",
            "init",
            "githooks",
            "check",
            "test",
            "test-fast",
            "test-slow",
            "test-all",
            "lint",
            "fmt",
            "fmt-check",
            "coverage",
            "doc-coverage",
            "validation",
            "validation-report",
            "check-grants",
            "check-keys",
            "ci",
            "ci-fast",
            "ci-full",
            "pre-commit",
            "ci-report",
            "ci-check",
            "branch-class",
            "fingerprint",
            "clean",
        ],
    )
    def test_the_target_exists(self, makefile, target):
        assert f"\n{target}:" in makefile

    def test_every_target_documents_itself(self, makefile):
        """`make help` is generated from the `##` comments, so an undocumented target is invisible."""
        undocumented = [
            line.split(":")[0]
            for line in makefile.splitlines()
            if re.match(r"^[a-z][a-z0-9-]*:", line)
            and "##" not in line
            and not line.startswith(("pre-commit:", "ci-fast:", "ci-full:"))
        ]
        assert undocumented == []

    def test_the_names_are_the_engine_makefile_s_names(self, makefile):
        """A person who has learnt one repository can work in the other without looking anything up.

        The engine's surface is laid out on two axes and this one has no second axis, so there are
        fewer targets here — but not *different* ones. A name that meant one thing there and another
        here would be worse than a name that does not exist.
        """
        for target in (
            "test-fast",
            "test-slow",
            "test-all",
            "ci-fast",
            "ci-full",
            "ci-check",
        ):
            assert f"\n{target}:" in makefile

    def test_the_names_it_records_are_the_ones_the_registry_knows(self, makefile):
        """A typo here would create a suite nothing has a policy for, reported under a name the
        gate has never heard of — which is the one failure a registry exists to prevent."""
        from freeports_dev.ci import metrics, suites

        for suite in suites.known_in(metrics.FORMATS):
            assert f",{suite.name})" in makefile

    def test_each_half_of_the_suite_is_selected_by_the_flag_and_not_by_a_marker(
        self, makefile
    ):
        """`make test-fast` and `freeports-dev test --fast` have to be the same run.

        The marker string used to be written out here *and* in the hook *and* in the registry's
        `command` field. One spelling of the selection, in the command that owns it, is what keeps
        the two surfaces from drifting apart.
        """
        assert "test $(REPO) $(FORMAT_ARG) --fast" in makefile
        assert "test $(REPO) $(FORMAT_ARG) --slow" in makefile
        assert "integration_tests" not in makefile

    def test_every_command_is_told_which_repository_it_acts_on(self, makefile):
        """Left to resolve it themselves, either command can be pointed elsewhere by a stray tier.

        A relative `FREEPORTS_FORMATS_REPO_PATH` carried in from another directory, or a
        `formats_repo:` line in a configuration file found from here, both name a different
        repository than the one being worked on — and the failure reads as the repository not being
        a repository, with its last path segment doubled.
        """
        # An invocation is the variable followed by a subcommand. `doctor` lists the same variable
        # beside the others to ask whether each is installed, which is a different thing entirely.
        invocation = re.compile(r"\$\(FREEPORTS_DEV\) [a-z][a-z-]*")
        for line in makefile.splitlines():
            if line.lstrip().startswith("#") or "command -v" in line:
                continue
            if invocation.search(line):
                assert "$(REPO)" in line, line

    def test_the_verdict_and_the_fingerprint_are_the_only_steps_that_refuse(
        self, makefile
    ):
        """Everything else measures and records; only the step that can see the branch decides.

        `GATE` is what expresses it: the two aggregates set it, GNU make hands a target-specific
        variable down to every prerequisite, and under it a failing suite or an unvouched-for grant
        is recorded rather than raised.
        """
        assert "ci-fast: GATE := 1" in makefile
        assert "ci-full: GATE := 1" in makefile
        assert makefile.count('test -n "$(GATE)"') >= 2

    def test_nothing_that_reaches_the_network_is_in_the_fast_gate(self, makefile):
        """A commit has to be possible on a train, and six seconds of somebody else's server is the
        single most expensive thing a per-commit gate could do."""
        fast = makefile[makefile.index("\nci-fast: lint") :].split("\n")[1]
        for network in ("validation", "check-grants", "check-keys"):
            assert network not in fast

    def test_and_all_of_it_is_in_the_prod_gate(self, makefile):
        """On a production branch nothing may be stale, so everything gated is established there."""
        full = makefile[makefile.index("\nci-full: lint") :].split("\n")[1]
        assert "validation" in full
        assert "test-all" in full

    def test_the_report_is_written_before_anything_may_refuse(self, makefile):
        """A run refused before the report was written leaves the badges describing the previous
        commit — the one arrangement in which a published figure is wrong and nothing says so."""
        for gate in ("ci-fast: lint", "ci-full: lint"):
            line = makefile[makefile.index(f"\n{gate}") :].split("\n")[1]
            assert line.index("ci-report") < line.index("fingerprint")
            assert line.index("ci-report") < line.index("ci-check")

    def test_a_page_is_rewritten_only_when_it_asks_to_be(self, makefile):
        """Both reports write *between markers*, so a file without them is not one this can fill.

        Testing for the file rather than for the markers is how one page that is not what it looks
        like takes the other eight renderings down with it.
        """
        assert "CI_MARKER" in makefile and "GRANT_MARKER" in makefile
        assert makefile.count('grep -q "$(CI_MARKER)"') == 1
        assert makefile.count('grep -q "$(GRANT_MARKER)"') == 2

    def test_the_report_is_not_rewritten_from_a_walk_that_resolved_nothing(
        self, makefile
    ):
        """Otherwise a network outage is committed as `coverage --, check-grants failing`, which is
        a claim about somebody's web server written into tracked files."""
        assert '"state": *"unmeasured"' in makefile
        assert "no methodology page could be resolved" in makefile

    def test_every_rendering_is_asked_for_in_one_invocation(self, makefile):
        """Five invocations are five interpreter starts, and five evaluations that could disagree
        with one another about the same run."""
        assert makefile.count("$(FREEPORTS_DEV) ci-report") == 1
        assert "--render badges:" in makefile

    def test_the_grant_walk_happens_once(self, makefile):
        """Nine artefacts out of nine collections would resolve every page nine times over."""
        assert makefile.count("collect >") == 1
        assert "--model" in makefile

    def test_it_forces_no_methodology_source_of_its_own(self, makefile):
        """The published `stable` channel is the command's own default, and the right one: it is
        what a reader of this repository can fetch, and a grant is a claim addressed to that reader.

        Naming a source here would take the decision away from whoever is running it, and silently:
        a ``--source`` on the command line outranks a configuration file, so a pattern written down
        in a generated Makefile would override the ``validate.sources`` somebody set in their own
        ``freeports-conf.yaml`` — the one tier where a personal choice belongs. Working against
        ``latest``, the channel built from the engine's branch, is exactly such a choice.
        """
        assert "\nVALIDATE_SOURCES ?=\n" in makefile
        assert "VALIDATE_SOURCE_ARGS" in makefile
        # Only in the comment that shows where such a choice belongs — never in a recipe or an
        # assignment, which is what would actually be passed to the command.
        for line in makefile.splitlines():
            if not line.lstrip().startswith("#"):
                assert "_sources/validation" not in line, line

    def test_the_measurements_are_not_carried_in_the_history(self, repo):
        """`reports/` is one machine's answer about one commit, and the Makefile says it is ignored."""
        assert "/reports/" in (repo / ".gitignore").read_text()


class TestTheHook:
    """What is left of the hook once the Makefile owns the gate: four checks and a target name."""

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

    def test_the_gate_is_a_name_and_not_a_list(self, hook):
        """What the gate consists of is decided in the Makefile, so it can grow without this file
        being edited again. The hook used to run a dozen commands, every one of them a second copy
        of something a person also had to be able to run by hand."""
        assert "make -C" in hook
        for own_work in (
            "freeports-dev test",
            "freeports-dev coverage",
            "freeports-dev lint-score",
            "freeports-validate --repo",
            "ci-record",
            "--render",
        ):
            assert own_work not in hook

    def test_it_runs_the_dev_gate_by_name(self, hook):
        assert "pre-commit" in hook

    def test_and_the_prod_gate_on_a_prod_branch(self, hook):
        invocation = [
            line for line in hook.splitlines() if line.lstrip().startswith("make -C")
        ]
        assert len(invocation) == 2
        prod, dev = invocation
        assert prod.endswith("ci-full || gate_status=1")
        assert dev.endswith("pre-commit || gate_status=1")
        assert 'if [ "$branch_class" = "prod" ]; then' in hook[: hook.index(prod)]

    def test_it_keeps_going_so_that_both_refusals_are_seen(self, hook):
        """Two steps of the gate can refuse. Without `-k` the first hides the second, and the next
        commit discovers it after fixing the first."""
        assert "-k" in hook

    def test_it_names_the_repository_git_is_committing(self, hook):
        """A hook has no business asking where it is: git already knows."""
        assert "git rev-parse --show-toplevel" in hook
        for line in hook.splitlines():
            if line.lstrip().startswith("make "):
                assert '-C "$repo_root"' in line, line

    def test_it_stages_what_the_run_rewrote(self, hook):
        """Otherwise the refresh is a dirty working tree rather than part of the commit."""
        assert 'git -C "$repo_root" add' in hook

    def test_it_stages_after_the_gate_and_not_before_it(self, hook):
        """The gate *is* what measures this commit. Anything staged before it describes the previous
        one and is rewritten a moment later — into the working tree, not into the commit."""
        assert hook.index("make -C") < hook.index('git -C "$repo_root" add')

    def test_it_stages_the_manifest_the_fingerprint_rule_may_have_rewritten(self, hook):
        assert "package.yaml" in hook

    def test_the_grant_report_is_staged_only_where_it_was_regenerated(self, hook):
        """`ci-full` runs `make validation`; `ci-fast` does not. What a run rewrote, that run stages."""
        block = hook[hook.index("refreshed=") :]
        index = block.index("validation/report")
        assert 'if [ "$branch_class" = "prod" ]; then' in block[:index]

    def test_the_only_refusal_is_the_gate_s_own_verdict(self, hook):
        """Whether a commit is refused is `ci-check`'s decision and depends on the branch, not on
        anything written in this file."""
        after_gate = hook[hook.index("gate_status=1") :]
        assert after_gate.count("exit 1") == 1
        assert "${gate_status:-}" in after_gate

    def test_and_it_refuses_on_a_prod_branch_only(self, hook):
        """On `dev` a failing gate is reported and the commit stands.

        The two steps that are meant to refuse already know the branch. A failing `make` on a dev
        branch therefore means something else went wrong — a missing tool, a measurement that
        crashed — which is worth saying loudly and not worth refusing a commit over.
        """
        refusal = hook[hook.index("${gate_status:-}") :]
        assert refusal.index('[ "$branch_class" = "prod" ]') < refusal.index("exit 1")
        assert "the commit stands" in refusal

    def test_an_off_branch_is_checked_not_at_all(self, hook):
        assert '[ "$branch_class" = "off" ]' in hook
        assert hook.index('"off"') < hook.index("make -C")

    def test_a_branch_nobody_classified_is_dev_and_never_off(self, hook):
        """The quiet answer is never the default one."""
        assert "branch_class=${branch_class:-dev}" in hook

    def test_it_is_shorter_than_the_makefile_it_calls(self, repo):
        """The point of the whole arrangement: the gate is described once, where it can be run by
        hand, and the hook is the thin thing that names it."""
        hook = (repo / ".githooks" / "pre-commit").read_text()
        assert len(hook) < len((repo / "Makefile").read_text())


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

    def test_the_gate_refreshes_it_and_the_hook_stages_it(self, repo):
        """The refresh moved into `make ci-report`, which both gates run before anything may refuse.

        What is left in the hook is the staging, and the staging cannot fail a commit: everything
        between the gate and the verdict is a `git add` of files the run rewrote.
        """
        makefile = (repo / "Makefile").read_text()
        for gate in ("ci-fast: lint", "ci-full: lint"):
            assert "ci-report" in makefile[makefile.index(gate) :].split("\n")[0]

        hook = (repo / ".githooks" / "pre-commit").read_text()
        block = hook[hook.index("refreshed=") : hook.index("${gate_status:-}")]
        assert "ci/report/badges" in block
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
