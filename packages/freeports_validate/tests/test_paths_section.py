"""The `Supported paths` section: what a methodology says it covers, and what follows from it.

A methodology page may declare which repository paths it applies to, in a section titled
``Supported paths`` whose entries are inline literals with a prose gloss underneath. The gloss is
the substance -- ``tests/formats/`` means one thing in a formats repository and another in the
engine's own, and the plan's decision is that the ambiguity is resolved *in prose* rather than
mechanically -- so the tool never tries to interpret it and always shows it.

Three behaviours follow from the declaration, and they are deliberately asymmetric:

``grant``
    refuses a path outside the declared set. Refusing at grant time is cheap and catches the mistake
    while its author is standing there, so the refusal is overridable -- by ``--force``, or by
    answering the question the command asks when a person is at a terminal.

``check-grants``
    warns and nothing more. Failing here would break a repository over a methodology page somebody
    else edited, which is not a thing a repository's own continuous integration should be able to
    suffer.

the three lookups
    list the grant, and mark it.

**Absence of the section means the methodology supports any path** -- today's behaviour, and the
honest answer for a methodology whose scope cannot be written as a set of paths.
"""

import json
import textwrap

import pytest

from conftest import with_supported_paths


PAGE_WITH_NO_SECTION = """\
Basic check
===========

The lightest of the methodologies.
"""

OUT_PATTERN = "tests/formats/*/*/out/*.csv"
PAGES_PATTERN = "tests/formats/*/*/pages/**"

OUT_PROSE = "The reference output of a format's test suite."
PAGES_PROSE = "The per-page fixtures of the same suite."

#: A path the fixture repository really contains, and which `OUT_PATTERN` really matches. The
#: variant level between the format and its outputs is easy to leave out of a pattern written from
#: memory, and both the fixture and these constants keep it.
GRANTED_OUT_FILE = "tests/formats/FOO-EN24/1/out/funds.csv"
OUTSIDE_FILE = "content/FOO/EN24.py"


def declaring(*entries):
    """A `basic check` page declaring the given entries."""
    return with_supported_paths(PAGE_WITH_NO_SECTION, *entries)


DECLARING_BOTH = declaring((OUT_PATTERN, OUT_PROSE), (PAGES_PATTERN, PAGES_PROSE))


# ---------------------------------------------------------------------------
# 2.1 -- reading the section
# ---------------------------------------------------------------------------


def parse(rst_paths, text):
    return rst_paths.supported_paths(text)


class TestWhetherTheSectionIsThereAtAll:
    """The distinction the whole feature rests on: undeclared is not the same as declaring nothing."""

    def test_a_page_without_the_section_declares_nothing(self, rst_paths):
        assert parse(rst_paths, PAGE_WITH_NO_SECTION)["declared"] is False

    def test_a_page_with_the_section_declares(self, rst_paths):
        assert parse(rst_paths, DECLARING_BOTH)["declared"] is True

    def test_an_empty_section_still_counts_as_declaring(self, rst_paths):
        """A section with no entries is a page saying something, even if it says it badly.

        Reading it as "supports anything" would let a grant through under a page whose author
        plainly meant to constrain it, and silence is the one answer that cannot be right here.
        """
        parsed = parse(rst_paths, declaring())
        assert parsed["declared"] is True
        assert parsed["patterns"] == []

    def test_a_section_of_prose_only_declares_no_pattern(self, rst_paths):
        page = PAGE_WITH_NO_SECTION + textwrap.dedent(
            """
            Supported paths
            ===============

            This methodology is about assertions in the documentation, and the set of files it
            covers is not one a path pattern can describe.
            """
        )
        parsed = parse(rst_paths, page)
        assert parsed["declared"] is True
        assert parsed["patterns"] == []

    def test_the_heading_is_matched_regardless_of_case(self, rst_paths):
        """`Supported Paths` is unmistakably the same section, and quietly ignoring it would be
        the worst of the three possible readings: the page would constrain nothing and say so
        nowhere."""
        page = with_supported_paths(
            PAGE_WITH_NO_SECTION, (OUT_PATTERN, OUT_PROSE), heading="Supported Paths"
        )
        assert parse(rst_paths, page)["declared"] is True

    def test_a_section_merely_mentioning_the_words_is_not_the_section(self, rst_paths):
        page = PAGE_WITH_NO_SECTION + textwrap.dedent(
            """
            About the supported paths of other methodologies
            ================================================

            ``tests/formats/*/*/out/*.csv``
                Not a declaration: this is a section about something else.
            """
        )
        assert parse(rst_paths, page)["declared"] is False


class TestReadingTheEntries:
    def test_each_entry_is_a_pattern_and_its_gloss(self, rst_paths):
        patterns = parse(rst_paths, DECLARING_BOTH)["patterns"]
        assert [entry["pattern"] for entry in patterns] == [OUT_PATTERN, PAGES_PATTERN]
        assert [entry["prose"] for entry in patterns] == [OUT_PROSE, PAGES_PROSE]

    def test_a_gloss_of_several_lines_is_kept_whole(self, rst_paths):
        prose = (
            "The reference output of a format's test suite.\nA human has looked at it."
        )
        patterns = parse(rst_paths, declaring((OUT_PATTERN, prose)))["patterns"]
        assert patterns[0]["prose"] == prose

    def test_a_gloss_of_several_paragraphs_is_kept_whole(self, rst_paths):
        prose = "First paragraph.\n\nSecond paragraph."
        patterns = parse(rst_paths, declaring((OUT_PATTERN, prose)))["patterns"]
        assert patterns[0]["prose"] == prose

    def test_the_gloss_is_dedented_to_its_own_left_margin(self, rst_paths):
        """What the reader is shown is prose, not prose with the page's indentation baked in."""
        patterns = parse(rst_paths, DECLARING_BOTH)["patterns"]
        assert not patterns[0]["prose"].startswith(" ")

    def test_a_pattern_with_no_gloss_is_still_a_pattern(self, rst_paths):
        """A page that declares a path without saying what it means is a page to improve, not a
        declaration to discard: dropping the entry would silently widen what the methodology
        covers."""
        patterns = parse(rst_paths, declaring((OUT_PATTERN,)))["patterns"]
        assert [entry["pattern"] for entry in patterns] == [OUT_PATTERN]
        assert patterns[0]["prose"] == ""

    def test_prose_between_the_entries_is_ignored(self, rst_paths):
        page = PAGE_WITH_NO_SECTION + textwrap.dedent(
            f"""
            Supported paths
            ===============

            Vouching for a file under this methodology means the protocol above was applied to it.

            ``{OUT_PATTERN}``
                {OUT_PROSE}

            The paths below are the ones added most recently.

            ``{PAGES_PATTERN}``
                {PAGES_PROSE}
            """
        )
        patterns = parse(rst_paths, page)["patterns"]
        assert [entry["pattern"] for entry in patterns] == [OUT_PATTERN, PAGES_PATTERN]

    def test_an_inline_literal_inside_a_sentence_is_not_an_entry(self, rst_paths):
        page = PAGE_WITH_NO_SECTION + textwrap.dedent(
            f"""
            Supported paths
            ===============

            Paths such as ``{PAGES_PATTERN}`` are covered elsewhere.

            ``{OUT_PATTERN}``
                {OUT_PROSE}
            """
        )
        patterns = parse(rst_paths, page)["patterns"]
        assert [entry["pattern"] for entry in patterns] == [OUT_PATTERN]


class TestWhereTheSectionEnds:
    def test_a_following_section_of_the_same_level_ends_it(self, rst_paths):
        page = DECLARING_BOTH + textwrap.dedent(
            """
            Limits
            ======

            ``not/a/declaration``
                This belongs to the section after.
            """
        )
        patterns = parse(rst_paths, page)["patterns"]
        assert [entry["pattern"] for entry in patterns] == [OUT_PATTERN, PAGES_PATTERN]

    def test_a_following_section_of_a_higher_level_ends_it(self, rst_paths):
        """`Supported paths` written as a subsection, closed by the next top-level heading."""
        page = textwrap.dedent(
            f"""
            Basic check
            ===========

            Supported paths
            ---------------

            ``{OUT_PATTERN}``
                {OUT_PROSE}

            Limits
            ======

            ``not/a/declaration``
                This belongs to the section after.
            """
        )
        patterns = parse(rst_paths, page)["patterns"]
        assert [entry["pattern"] for entry in patterns] == [OUT_PATTERN]

    def test_a_subsection_of_its_own_does_not_end_it(self, rst_paths):
        page = textwrap.dedent(
            f"""
            Basic check
            ===========

            Supported paths
            ===============

            ``{OUT_PATTERN}``
                {OUT_PROSE}

            In a formats repository
            -----------------------

            ``{PAGES_PATTERN}``
                {PAGES_PROSE}
            """
        )
        patterns = parse(rst_paths, page)["patterns"]
        assert [entry["pattern"] for entry in patterns] == [OUT_PATTERN, PAGES_PATTERN]

    def test_the_end_of_the_page_ends_it(self, rst_paths):
        assert len(parse(rst_paths, DECLARING_BOTH)["patterns"]) == 2

    def test_an_overlined_title_is_recognised(self, rst_paths):
        page = textwrap.dedent(
            f"""
            ===============
            Supported paths
            ===============

            ``{OUT_PATTERN}``
                {OUT_PROSE}
            """
        )
        assert parse(rst_paths, page)["declared"] is True


class TestTheParserAsAProcess:
    """The contract `lib/paths.sh` depends on: the page on stdin, JSON on stdout."""

    def test_it_reads_stdin_and_writes_json(self, rst_paths_script):
        run = rst_paths_script(stdin=DECLARING_BOTH)
        assert run.returncode == 0, run
        parsed = json.loads(run.stdout)["supported_paths"]
        assert parsed["declared"] is True
        assert parsed["patterns"][0]["pattern"] == OUT_PATTERN

    def test_a_page_with_no_section_is_not_an_error(self, rst_paths_script):
        run = rst_paths_script(stdin=PAGE_WITH_NO_SECTION)
        assert run.returncode == 0, run
        assert json.loads(run.stdout)["supported_paths"]["declared"] is False

    def test_an_empty_page_is_not_an_error(self, rst_paths_script):
        run = rst_paths_script(stdin="")
        assert run.returncode == 0, run
        assert json.loads(run.stdout)["supported_paths"]["declared"] is False


# ---------------------------------------------------------------------------
# 2.2 -- the pattern grammar
# ---------------------------------------------------------------------------


class TestLiteralPatterns:
    def test_a_literal_path_matches_itself(self, pathmatch):
        assert pathmatch.matches("README.md", "README.md")

    def test_a_literal_path_matches_nothing_else(self, pathmatch):
        assert not pathmatch.matches("README.md", "docs/README.md")

    def test_matching_is_case_sensitive(self, pathmatch):
        assert not pathmatch.matches("README.md", "readme.md")

    def test_a_leading_slash_is_ignored_rather_than_refused(self, pathmatch):
        """Patterns are relative to the repository root; a slash in front says the same thing."""
        assert pathmatch.matches("/tests/formats", "tests/formats")


class TestTheSingleStar:
    def test_it_matches_within_one_segment(self, pathmatch):
        assert pathmatch.matches("tests/*/out", "tests/FOO/out")

    def test_it_never_crosses_a_slash(self, pathmatch):
        assert not pathmatch.matches("tests/*/out", "tests/FOO/1/out")

    def test_it_matches_an_extension(self, pathmatch):
        assert pathmatch.matches("out/*.csv", "out/funds.csv")

    def test_an_extension_is_required_when_written(self, pathmatch):
        assert not pathmatch.matches("out/*.csv", "out/funds.json")

    def test_it_needs_something_to_match(self, pathmatch):
        """`*.csv` is a name with an extension, not a bare extension."""
        assert not pathmatch.matches("out/*.csv", "out/.csv")

    def test_several_stars_in_one_segment_are_allowed(self, pathmatch):
        assert pathmatch.matches("out/*-*.csv", "out/funds-2024.csv")


class TestTheDoubleStar:
    def test_it_matches_several_segments(self, pathmatch):
        assert pathmatch.matches("tests/**/funds.csv", "tests/FOO/1/out/funds.csv")

    def test_it_matches_one_segment(self, pathmatch):
        assert pathmatch.matches("tests/**/funds.csv", "tests/out/funds.csv")

    def test_it_matches_no_segment_at_all(self, pathmatch):
        assert pathmatch.matches("tests/**/funds.csv", "tests/funds.csv")

    def test_at_the_end_it_takes_everything_below(self, pathmatch):
        assert pathmatch.matches("tests/pages/**", "tests/pages/investments/1.json")

    def test_at_the_end_it_also_takes_the_directory_itself(self, pathmatch):
        assert pathmatch.matches("tests/pages/**", "tests/pages")

    def test_at_the_start_it_takes_any_prefix(self, pathmatch):
        assert pathmatch.matches("**/funds.csv", "tests/FOO/1/out/funds.csv")

    def test_at_the_start_it_also_takes_no_prefix(self, pathmatch):
        assert pathmatch.matches("**/funds.csv", "funds.csv")


class TestTheTrailingSlash:
    def test_it_takes_everything_under_the_directory(self, pathmatch):
        assert pathmatch.matches("tests/", "tests/formats/FOO/1/out/funds.csv")

    def test_it_is_the_same_as_appending_a_double_star(self, pathmatch):
        for path in ("tests", "tests/a", "tests/a/b"):
            assert pathmatch.matches("tests/", path) == pathmatch.matches(
                "tests/**", path
            ), path

    def test_it_does_not_take_a_sibling_whose_name_merely_starts_the_same(
        self, pathmatch
    ):
        assert not pathmatch.matches("tests/", "tests-old/a")


class TestWhatTheGrammarDeliberatelyLacks:
    """The set is small on purpose: these patterns are read by people deciding whether to trust."""

    def test_a_character_class_is_literal(self, pathmatch):
        assert pathmatch.matches("out/[ab].csv", "out/[ab].csv")
        assert not pathmatch.matches("out/[ab].csv", "out/a.csv")

    def test_a_brace_expansion_is_literal(self, pathmatch):
        assert not pathmatch.matches("out/{a,b}.csv", "out/a.csv")

    def test_a_question_mark_is_literal(self, pathmatch):
        assert not pathmatch.matches("out/?.csv", "out/a.csv")

    def test_a_dot_is_a_dot_and_not_a_regular_expression(self, pathmatch):
        assert not pathmatch.matches("out/a.csv", "out/axcsv")


class TestExpandingAgainstATree:
    """The coverage denominator: what a pattern *could* cover in a repository as it stands."""

    def test_it_finds_the_files_a_pattern_covers(self, pathmatch, tmp_repo):
        found = pathmatch.expand(OUT_PATTERN, tmp_repo.root)
        assert found == [
            "tests/formats/FOO-EN24/1/out/funds.csv",
            "tests/formats/FOO-EN24/1/out/investments.csv",
        ]

    def test_it_answers_with_repository_relative_paths(self, pathmatch, tmp_repo):
        for path in pathmatch.expand("**", tmp_repo.root):
            assert not path.startswith("/")

    def test_it_finds_nothing_when_nothing_matches(self, pathmatch, tmp_repo):
        assert pathmatch.expand("does/not/exist/**", tmp_repo.root) == []

    def test_the_answer_is_ordered(self, pathmatch, tmp_repo):
        """A denominator that changed with the order `os.walk` happened to return would make a
        coverage figure differ between two runs over the same tree."""
        found = pathmatch.expand("**", tmp_repo.root)
        assert found == sorted(found)

    def test_the_repositorys_own_git_metadata_is_not_covered_by_anything(
        self, pathmatch, tmp_repo
    ):
        tmp_repo.write(".git/config", "[core]\n")
        assert ".git/config" not in pathmatch.expand("**", tmp_repo.root)

    def test_it_lists_files_and_not_directories(self, pathmatch, tmp_repo):
        assert "tests/formats" not in pathmatch.expand("**", tmp_repo.root)


class TestTheMatcherAsAProcess:
    """`match` is the retypable one; `filter` is the one the shell scripts actually call."""

    def test_match_exits_zero_when_the_path_is_covered(self, pathmatch_script):
        run = pathmatch_script("match", OUT_PATTERN, GRANTED_OUT_FILE)
        assert run.returncode == 0, run

    def test_match_exits_non_zero_when_it_is_not(self, pathmatch_script):
        run = pathmatch_script("match", OUT_PATTERN, OUTSIDE_FILE)
        assert run.returncode == 1, run

    def test_filter_names_the_pattern_a_path_matched(self, pathmatch_script):
        run = pathmatch_script(
            "filter",
            GRANTED_OUT_FILE,
            stdin=f"{PAGES_PATTERN}\n{OUT_PATTERN}\n",
        )
        assert run.returncode == 0, run
        assert json.loads(run.stdout) == [
            {"path": GRANTED_OUT_FILE, "matched": OUT_PATTERN}
        ]

    def test_filter_answers_null_for_a_path_no_pattern_covers(self, pathmatch_script):
        run = pathmatch_script("filter", OUTSIDE_FILE, stdin=f"{OUT_PATTERN}\n")
        assert json.loads(run.stdout) == [{"path": OUTSIDE_FILE, "matched": None}]

    def test_filter_answers_for_every_path_it_was_given(self, pathmatch_script):
        run = pathmatch_script(
            "filter", GRANTED_OUT_FILE, OUTSIDE_FILE, stdin=f"{OUT_PATTERN}\n"
        )
        assert [entry["path"] for entry in json.loads(run.stdout)] == [
            GRANTED_OUT_FILE,
            OUTSIDE_FILE,
        ]

    def test_expand_writes_json(self, pathmatch_script, tmp_repo):
        run = pathmatch_script("expand", OUT_PATTERN, tmp_repo.root)
        assert run.returncode == 0, run
        assert json.loads(run.stdout) == [
            "tests/formats/FOO-EN24/1/out/funds.csv",
            "tests/formats/FOO-EN24/1/out/investments.csv",
        ]

    def test_an_unknown_mode_is_refused(self, pathmatch_script):
        run = pathmatch_script("frobnicate", "a", "b")
        assert run.returncode != 0, run


# ---------------------------------------------------------------------------
# The seam: `lib/paths.sh` between the resolver and the two helpers
# ---------------------------------------------------------------------------


class TestTheShellSeam:
    def test_a_path_a_methodology_declares_is_supported(
        self, paths_lib, methodology_pages, local_source
    ):
        methodology_pages.write("methodologies/basic_check", DECLARING_BOTH)
        run = paths_lib(
            f'path_is_supported "basic check" "{GRANTED_OUT_FILE}"; echo "status=$?"',
            sources=local_source,
        )
        assert "status=0" in run.stdout, run

    def test_a_path_it_does_not_declare_is_unsupported(
        self, paths_lib, methodology_pages, local_source
    ):
        methodology_pages.write("methodologies/basic_check", DECLARING_BOTH)
        run = paths_lib(
            f'path_is_supported "basic check" "{OUTSIDE_FILE}"; echo "status=$?"',
            sources=local_source,
        )
        assert "status=1" in run.stdout, run

    def test_a_page_declaring_nothing_supports_everything(
        self, paths_lib, methodology_pages, local_source
    ):
        run = paths_lib(
            f'path_is_supported "basic check" "{OUTSIDE_FILE}"; echo "status=$?"',
            sources=local_source,
        )
        assert "status=0" in run.stdout, run

    def test_a_page_that_cannot_be_resolved_leaves_the_question_unanswered(
        self, paths_lib, local_source
    ):
        """Neither supported nor unsupported: a third answer, because there are three cases."""
        run = paths_lib(
            f'path_is_supported "no such methodology" "{OUTSIDE_FILE}"; echo "status=$?"',
            sources=local_source,
        )
        assert "status=2" in run.stdout, run

    def test_the_patterns_are_printed_with_their_prose(
        self, paths_lib, methodology_pages, local_source
    ):
        methodology_pages.write("methodologies/basic_check", DECLARING_BOTH)
        run = paths_lib('print_supported_paths "basic check"', sources=local_source)
        assert OUT_PATTERN in run.output, run
        assert OUT_PROSE in run.output, run
        assert PAGES_PATTERN in run.output, run
        assert PAGES_PROSE in run.output, run

    def test_the_page_is_resolved_once_however_often_it_is_asked_about(
        self, paths_lib, methodology_pages, http_source
    ):
        """A document may grant hundreds of files under one methodology, and the page they were
        granted under is the same page every time."""
        methodology_pages.write("methodologies/basic_check", DECLARING_BOTH)
        run = paths_lib(
            'for path in a b c d e; do path_is_supported "basic check" "$path"; done; echo done',
            sources=http_source.pattern,
        )
        assert "done" in run.stdout, run
        fetched = [
            path for path in http_source.requested if path.endswith("basic_check.rst")
        ]
        assert len(fetched) == 1, http_source.requested


# ---------------------------------------------------------------------------
# 2.3 -- `grant`
# ---------------------------------------------------------------------------


@pytest.fixture
def declared(methodology_pages):
    """`basic check`, published as a page that declares which paths it covers."""
    methodology_pages.write("methodologies/basic_check", DECLARING_BOTH)
    return methodology_pages


def build_adopted(repo, pages, run, signer):
    """A signed document that has adopted `basic check` -- the state a file grant starts from.

    Three invocations of the command: create, sign, adopt. The page it adopts is written here
    because the document records that text's hash, so the page and the adoption have to be raised
    together or the state is one no repository could be in.
    """
    pages.write("methodologies/basic_check", DECLARING_BOTH)
    common = {
        "repo": repo,
        "key_id": signer.fingerprint,
        "sources": pages.file_pattern,
    }
    for arguments in (
        ("create-document",),
        ("sign-document",),
        ("grant", "with", "basic check"),
    ):
        answer = run(*arguments, **common)
        assert answer.returncode == 0, answer


@pytest.fixture
def adopted(prototypes, tmp_path, tmp_repo, signer, declared):
    """The state above, built once for the session and copied here.

    It cost 4.3 s and was rebuilt by every test that asked for it -- five of them in
    `TestCheckGrantsOnAnOutOfScopeGrant` alone, all asserting different things about one state.
    See `Prototypes` in `conftest.py`.
    """
    prototypes.restore(build_adopted, tmp_path)
    return tmp_repo.document(signer)


@pytest.fixture
def grant(run_validate, tmp_repo, signer, local_source):
    """Grant one repository-relative path, with whatever extra arguments the test wants."""

    def call(relative, *extra, **kwargs):
        return run_validate(
            "grant",
            *extra,
            str(tmp_repo.root / relative),
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
            cwd=tmp_repo.root,
            **kwargs,
        )

    return call


class TestGrantingInsideTheDeclaredPaths:
    def test_a_declared_path_is_granted(self, grant, adopted):
        run = grant(GRANTED_OUT_FILE)
        assert run.returncode == 0, run
        assert GRANTED_OUT_FILE in adopted.read_text()

    def test_a_declared_path_is_not_questioned(self, grant, adopted):
        run = grant(GRANTED_OUT_FILE)
        assert "supported" not in run.output.lower(), run

    def test_a_page_declaring_nothing_grants_anything(
        self, run_validate, tmp_repo, signer, local_source, signed_document
    ):
        """The methodology whose page has no section at all: today's behaviour, unchanged."""
        run_validate(
            "grant",
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        run = run_validate(
            "grant",
            str(tmp_repo.root / OUTSIDE_FILE),
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
            cwd=tmp_repo.root,
        )
        assert run.returncode == 0, run
        assert OUTSIDE_FILE in signed_document.read_text()


class TestGrantingOutsideTheDeclaredPaths:
    def test_it_is_refused(self, grant, adopted):
        run = grant(OUTSIDE_FILE)
        assert run.returncode != 0, run

    def test_the_refusal_names_the_path_and_the_methodology(self, grant, adopted):
        run = grant(OUTSIDE_FILE)
        assert OUTSIDE_FILE in run.output, run
        assert "basic check" in run.output, run

    def test_the_refusal_shows_every_pattern_with_its_prose(self, grant, adopted):
        """ "Not supported" is unactionable; "this covers reference outputs" is what a person needs
        to decide whether they picked the wrong methodology or the wrong file."""
        run = grant(OUTSIDE_FILE)
        for text in (OUT_PATTERN, OUT_PROSE, PAGES_PATTERN, PAGES_PROSE):
            assert text in run.output, (text, run)

    def test_the_refusal_says_how_to_override_it(self, grant, adopted):
        run = grant(OUTSIDE_FILE)
        assert "--force" in run.output, run

    def test_nothing_is_written_to_the_document(self, grant, adopted):
        before = adopted.read_text()
        grant(OUTSIDE_FILE)
        assert adopted.read_text() == before

    def test_a_refused_file_takes_its_companions_down_with_it(
        self, run_validate, tmp_repo, signer, local_source, adopted
    ):
        """A grant is one act, signed once at the end. Adding the acceptable half and refusing the
        rest would leave a modified document that nobody had put their name to."""
        before = adopted.read_text()
        run = run_validate(
            "grant",
            str(tmp_repo.root / GRANTED_OUT_FILE),
            str(tmp_repo.root / OUTSIDE_FILE),
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
            cwd=tmp_repo.root,
        )
        assert run.returncode != 0, run
        assert adopted.read_text() == before

    def test_the_document_still_checks_out_afterwards(
        self, run_validate, tmp_repo, signer, local_source, grant, adopted
    ):
        grant(OUTSIDE_FILE)
        run = run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode == 0, run


class TestForcing:
    def test_force_grants_it_anyway(self, grant, adopted):
        run = grant(OUTSIDE_FILE, "--force")
        assert run.returncode == 0, run
        assert OUTSIDE_FILE in adopted.read_text()

    def test_force_still_says_what_it_overrode(self, grant, adopted):
        """Overriding a refusal in silence would make the declaration unenforceable *and*
        invisible, which is worse than not having it."""
        run = grant(OUTSIDE_FILE, "--force")
        assert OUT_PATTERN in run.output, run

    def test_force_does_not_disturb_an_ordinary_grant(self, grant, adopted):
        run = grant(GRANTED_OUT_FILE, "--force")
        assert run.returncode == 0, run


class TestBeingAsked:
    """The branch that only exists when a person is there to answer."""

    def test_a_terminal_is_asked_rather_than_refused(self, grant, adopted):
        run = grant(OUTSIDE_FILE, tty=True, stdin="y\n")
        assert run.returncode == 0, run
        assert OUTSIDE_FILE in adopted.read_text()

    def test_answering_no_refuses(self, grant, adopted):
        run = grant(OUTSIDE_FILE, tty=True, stdin="n\n")
        assert run.returncode != 0, run
        assert OUTSIDE_FILE not in adopted.read_text()

    def test_the_default_answer_is_no(self, grant, adopted):
        """A grant is a signature. The keystroke that costs nothing must be the one that
        withholds it."""
        run = grant(OUTSIDE_FILE, tty=True, stdin="\n")
        assert run.returncode != 0, run

    def test_a_pipe_is_not_asked(self, grant, adopted):
        """No terminal means no person, and a question nobody can answer must not become a
        default yes."""
        run = grant(OUTSIDE_FILE)
        assert run.returncode != 0, run
        assert OUTSIDE_FILE not in adopted.read_text()


class TestGrantingUnderAPageThatCannotBeRead:
    def test_it_is_refused(
        self, run_validate, tmp_repo, signer, methodology_pages, adopted
    ):
        """Vouching under a text you could not read is exactly what a signature must not mean."""
        methodology_pages.path("methodologies/basic_check").unlink()
        run = run_validate(
            "grant",
            str(tmp_repo.root / GRANTED_OUT_FILE),
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=methodology_pages.file_pattern,
            cwd=tmp_repo.root,
        )
        assert run.returncode != 0, run

    def test_force_grants_it_anyway(
        self, run_validate, tmp_repo, signer, methodology_pages, adopted
    ):
        methodology_pages.path("methodologies/basic_check").unlink()
        run = run_validate(
            "grant",
            "--force",
            str(tmp_repo.root / GRANTED_OUT_FILE),
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=methodology_pages.file_pattern,
            cwd=tmp_repo.root,
        )
        assert run.returncode == 0, run


# ---------------------------------------------------------------------------
# 2.4 -- `check-grants`
# ---------------------------------------------------------------------------


def build_granted_outside(repo, pages, run, signer):
    """A document holding a grant on a path its methodology does not declare.

    Made with `--force`, which is the only honest way to reach this state through the command --
    and is exactly how a repository reaches it in life, since the other way is a page that gained a
    `Supported paths` section after the grant was signed.
    """
    build_adopted(repo, pages, run, signer)
    answer = run(
        "grant",
        "--force",
        str(repo.root / OUTSIDE_FILE),
        "with",
        "basic check",
        repo=repo,
        key_id=signer.fingerprint,
        sources=pages.file_pattern,
        cwd=repo.root,
    )
    assert answer.returncode == 0, answer


@pytest.fixture
def granted_outside(prototypes, tmp_path, tmp_repo, signer, declared):
    """The state above, built once and copied -- five tests in one class wanted it."""
    prototypes.restore(build_granted_outside, tmp_path)
    return tmp_repo.document(signer)


class TestCheckGrantsOnAnOutOfScopeGrant:
    @pytest.fixture
    def checked(self, run_validate, tmp_repo, signer, local_source, granted_outside):
        return run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )

    def test_the_run_still_passes(self, checked):
        """Failing here would break a repository over a page somebody else edited."""
        assert checked.returncode == 0, checked

    def test_the_grant_is_reported(self, checked):
        assert OUTSIDE_FILE in checked.output, checked

    def test_it_is_reported_as_a_warning(self, checked):
        assert "⚠" in checked.output, checked

    def test_the_warning_is_explained_at_the_end_of_the_run(self, checked):
        """Every failure kind this command reports gets its paragraph; a warning nobody explains
        is a warning nobody acts on."""
        assert "What these errors mean" in checked.output, checked
        assert "grant" in checked.output.lower(), checked

    def test_the_explanation_shows_what_the_methodology_does_declare(self, checked):
        assert OUT_PATTERN in checked.output, checked


class TestCheckGrantsOnAnInScopeGrant:
    @pytest.fixture
    def checked(
        self, run_validate, tmp_repo, signer, local_source, grant, adopted, declared
    ):
        assert grant(GRANTED_OUT_FILE).returncode == 0
        return run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )

    def test_the_run_passes(self, checked):
        assert checked.returncode == 0, checked

    def test_nothing_is_said_about_supported_paths(self, checked):
        assert OUT_PATTERN not in checked.output, checked


class TestCheckGrantsWhenTheDeclarationCannotBeRead:
    def test_an_unreachable_page_does_not_produce_a_path_warning(
        self, run_validate, tmp_repo, signer, methodology_pages, grant, adopted
    ):
        """The page is what says which paths are declared; without it there is no question to
        answer, and the run already reports the unreachable page on its own line."""
        assert grant(GRANTED_OUT_FILE).returncode == 0
        methodology_pages.path("methodologies/basic_check").unlink()
        run = run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=methodology_pages.file_pattern,
        )
        assert "declares" not in run.output.lower(), run


# ---------------------------------------------------------------------------
# 2.5 -- the three lookups
# ---------------------------------------------------------------------------


class TestTheLookupsAnnotate:
    """A lookup lists a grant whatever its scope -- somebody made that claim -- and marks it."""

    MARK = "(!)"

    def test_who_grants_marks_the_grant(
        self, run_validate, tmp_repo, signer, local_source, granted_outside
    ):
        run = run_validate(
            "who-grants",
            str(tmp_repo.root / OUTSIDE_FILE),
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
            cwd=tmp_repo.root,
        )
        assert run.returncode == 0, run
        assert signer.name in run.output, run
        assert self.MARK in run.output, run

    def test_who_grants_explains_the_mark(
        self, run_validate, tmp_repo, signer, local_source, granted_outside
    ):
        run = run_validate(
            "who-grants",
            str(tmp_repo.root / OUTSIDE_FILE),
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
            cwd=tmp_repo.root,
        )
        assert "declares" in run.output.lower(), run

    def test_granted_by_marks_the_grant(
        self, run_validate, tmp_repo, signer, local_source, granted_outside
    ):
        run = run_validate(
            "granted-by",
            signer.email,
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode == 0, run
        assert OUTSIDE_FILE in run.output, run
        assert self.MARK in run.output, run

    def test_granted_with_marks_the_grant(
        self, run_validate, tmp_repo, signer, local_source, granted_outside
    ):
        run = run_validate(
            "granted-with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode == 0, run
        assert OUTSIDE_FILE in run.output, run
        assert self.MARK in run.output, run

    def test_an_in_scope_grant_is_not_marked(
        self, run_validate, tmp_repo, signer, local_source, grant, adopted, declared
    ):
        assert grant(GRANTED_OUT_FILE).returncode == 0
        run = run_validate(
            "granted-with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode == 0, run
        assert GRANTED_OUT_FILE in run.output, run
        assert self.MARK not in run.output, run
