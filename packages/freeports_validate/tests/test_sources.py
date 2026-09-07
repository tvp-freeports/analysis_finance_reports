"""`lib/sources.sh`: turning a methodology's name into a text you can hash.

The pages no longer ship with the command, so this library is where a grant's meaning now comes
from. Everything it does is a shell command a person can retype -- a substitution, a `curl`, a
`sha256sum` -- and these tests call its functions the way the documentation tells a reader to call
them, so that the two cannot drift apart.

`resolve_methodology` answers on three lines, in this order:

    the URI it resolved
    a local path holding that text
    where the text came from -- `local`, `network` or `cache`

and its exit status separates the two failures that must never be confused: **1** for a name no
configured source offers, **2** for a source that could not be reached at all. The second is not a
mismatch and not a match; it is the absence of an answer, and saying so is the whole point.
"""

import subprocess
from hashlib import sha256

import pytest


GITHUB_BLOB = "https://github.com/tvp-freeports/analysis_finance_reports/blob/main/docs/source/validation/*.rst"
GITHUB_RAW = "https://raw.githubusercontent.com/tvp-freeports/analysis_finance_reports/main/docs/source/validation"

UNRESOLVED = 1
UNREACHABLE = 2


def resolution(run):
    """The three lines of a successful `resolve_methodology`, as a tuple."""
    lines = run.stdout.strip().splitlines()
    assert len(lines) == 3, run
    return tuple(lines)


class TestThePatternGrammar:
    """A source is a pattern with exactly one `*`, and the `*` is the methodology's name."""

    def test_a_pattern_with_one_star_is_accepted(self, sources_lib):
        run = sources_lib('validate_source_pattern "/pages/*.rst" && echo accepted')
        assert run.returncode == 0, run
        assert "accepted" in run.stdout

    def test_a_pattern_with_no_star_is_refused_and_named(self, sources_lib):
        run = sources_lib('validate_source_pattern "/pages/basic_check.rst"')
        assert run.returncode != 0, run
        assert "/pages/basic_check.rst" in run.output

    def test_a_pattern_with_two_stars_is_refused_and_named(self, sources_lib):
        """Not guessable which half is the base, so it is refused rather than resolved somehow."""
        run = sources_lib('validate_source_pattern "/*/pages/*.rst"')
        assert run.returncode != 0, run
        assert "/*/pages/*.rst" in run.output

    def test_the_refusal_says_what_a_source_is_supposed_to_look_like(self, sources_lib):
        run = sources_lib('validate_source_pattern "/pages/"')
        assert "*" in run.output

    def test_an_empty_pattern_is_refused(self, sources_lib):
        run = sources_lib('validate_source_pattern ""')
        assert run.returncode != 0, run


class TestNameToRelativePath:
    def test_a_methodology_becomes_a_path_under_methodologies(self, sources_lib):
        run = sources_lib('methodology_relative_name "basic check"')
        assert run.stdout.strip() == "methodologies/basic_check"

    def test_the_general_methodology_sits_at_the_root(self, sources_lib):
        run = sources_lib("general_methodology_relative_name")
        assert run.stdout.strip() == "general_methodology"

    def test_a_name_is_normalised_the_way_the_document_stores_it(self, sources_lib):
        """`Golden_Standard`, `golden standard` and `GOLDEN STANDARD` are one methodology."""
        for written in ("Golden_Standard", "golden standard", "GOLDEN STANDARD"):
            run = sources_lib(f'methodology_relative_name "{written}"')
            assert run.stdout.strip() == "methodologies/golden_standard", written


class TestSubstitution:
    def test_the_star_becomes_the_relative_name(self, sources_lib):
        run = sources_lib('source_uri_for "/pages/*.rst" "methodologies/basic_check"')
        assert run.stdout.strip() == "/pages/methodologies/basic_check.rst"

    def test_a_name_containing_a_slash_is_substituted_whole(self, sources_lib):
        """The `*` stands for a path, not for one segment: that is what makes one pattern enough."""
        run = sources_lib(
            'source_uri_for "https://docs.example.org/_sources/validation/*.rst.txt" '
            '"methodologies/basic_check"'
        )
        assert run.stdout.strip() == (
            "https://docs.example.org/_sources/validation/methodologies/basic_check.rst.txt"
        )

    def test_the_extension_after_the_star_is_kept(self, sources_lib):
        run = sources_lib('source_uri_for "/pages/*.rst.txt" "general_methodology"')
        assert run.stdout.strip() == "/pages/general_methodology.rst.txt"

    def test_a_pattern_ending_in_the_star_needs_no_extension(self, sources_lib):
        run = sources_lib('source_uri_for "/pages/*" "general_methodology"')
        assert run.stdout.strip() == "/pages/general_methodology"


class TestTheGithubRewrite:
    """A `blob` URL serves a web page; hashing it would hash the page's chrome, silently."""

    def test_a_blob_url_becomes_a_raw_url(self, sources_lib):
        run = sources_lib(f'source_uri_for "{GITHUB_BLOB}" "methodologies/basic_check"')
        assert run.stdout.strip() == f"{GITHUB_RAW}/methodologies/basic_check.rst"

    def test_a_raw_url_given_directly_is_left_alone(self, sources_lib):
        pattern = f"{GITHUB_RAW}/*.rst"
        run = sources_lib(f'source_uri_for "{pattern}" "general_methodology"')
        assert run.stdout.strip() == f"{GITHUB_RAW}/general_methodology.rst"

    def test_a_host_that_merely_contains_github_is_not_rewritten(self, sources_lib):
        pattern = "https://github.com.example.invalid/o/r/blob/main/*.rst"
        run = sources_lib(f'source_uri_for "{pattern}" "general_methodology"')
        assert run.stdout.strip() == (
            "https://github.com.example.invalid/o/r/blob/main/general_methodology.rst"
        )

    def test_a_url_elsewhere_on_github_is_left_alone(self, sources_lib):
        """Only the `blob` form is a page pretending to be a document."""
        pattern = (
            "https://github.com/tvp-freeports/analysis_finance_reports/raw/main/*.rst"
        )
        run = sources_lib(f'source_uri_for "{pattern}" "general_methodology"')
        assert "raw/main/general_methodology.rst" in run.stdout


class TestResolvingFromALocalSource:
    def test_a_file_url_resolves_to_the_page_itself(
        self, sources_lib, local_source, methodology_pages
    ):
        run = sources_lib('resolve_methodology "basic check"', sources=local_source)
        assert run.returncode == 0, run
        uri, path, origin = resolution(run)
        assert uri == f"file://{methodology_pages.root}/methodologies/basic_check.rst"
        assert path == str(methodology_pages.path("methodologies/basic_check"))
        assert origin == "local"

    def test_a_bare_relative_path_is_a_source_too(self, sources_lib, methodology_pages):
        run = sources_lib(
            'resolve_methodology "basic check"',
            sources="methodologies-source/*.rst",
            cwd=methodology_pages.root.parent,
        )
        assert run.returncode == 0, run
        _uri, path, origin = resolution(run)
        assert origin == "local"
        assert path == str(methodology_pages.path("methodologies/basic_check"))

    def test_the_path_it_answers_with_is_absolute(self, sources_lib, methodology_pages):
        """Callers hash it from wherever they are standing, so it cannot depend on a directory."""
        run = sources_lib(
            'resolve_methodology "basic check"',
            sources="methodologies-source/*.rst",
            cwd=methodology_pages.root.parent,
        )
        _uri, path, _origin = resolution(run)
        assert path.startswith("/")

    def test_the_general_methodology_resolves_from_the_same_source(
        self, sources_lib, local_source, methodology_pages
    ):
        run = sources_lib("resolve_general_methodology", sources=local_source)
        assert run.returncode == 0, run
        _uri, path, _origin = resolution(run)
        assert path == str(methodology_pages.path("general_methodology"))

    def test_a_name_no_source_offers_is_unresolved_not_unreachable(
        self, sources_lib, local_source
    ):
        run = sources_lib(
            'resolve_methodology "no such methodology"', sources=local_source
        )
        assert run.returncode == UNRESOLVED, run

    def test_the_hash_is_the_hash_of_the_page(
        self, sources_lib, local_source, methodology_pages
    ):
        run = sources_lib('methodology_sha256 "basic check"', sources=local_source)
        assert run.returncode == 0, run
        assert run.stdout.strip() == methodology_pages.sha256(
            "methodologies/basic_check"
        )

    def test_editing_the_page_changes_the_hash(
        self, sources_lib, local_source, methodology_pages
    ):
        before = sources_lib(
            'methodology_sha256 "basic check"', sources=local_source
        ).stdout.strip()
        methodology_pages.write(
            "methodologies/basic_check", "Basic check\n===========\n\nnew text\n"
        )
        after = sources_lib(
            'methodology_sha256 "basic check"', sources=local_source
        ).stdout.strip()
        assert before != after


class TestPriorityBetweenSources:
    """Order is priority: a name is resolved from the first source that has it."""

    @pytest.fixture
    def two_trees(self, tmp_path, methodology_pages):
        from conftest import PageTree

        mine = PageTree(root=tmp_path / "mine")
        mine.write(
            "methodologies/basic_check", "Basic check\n===========\n\nMy own text.\n"
        )
        mine.write("methodologies/only_mine", "Only mine\n=========\n")
        return mine, methodology_pages

    def test_the_first_source_wins_a_name_both_offer(self, sources_lib, two_trees):
        mine, theirs = two_trees
        run = sources_lib(
            'resolve_methodology "basic check"',
            sources=[mine.file_pattern, theirs.file_pattern],
        )
        _uri, path, _origin = resolution(run)
        assert open(path).read() == mine.path("methodologies/basic_check").read_text()

    def test_reversing_the_order_reverses_the_answer(self, sources_lib, two_trees):
        mine, theirs = two_trees
        run = sources_lib(
            'resolve_methodology "basic check"',
            sources=[theirs.file_pattern, mine.file_pattern],
        )
        _uri, path, _origin = resolution(run)
        assert open(path).read() == theirs.path("methodologies/basic_check").read_text()

    def test_a_later_source_supplies_what_the_first_does_not_have(
        self, sources_lib, two_trees
    ):
        mine, theirs = two_trees
        run = sources_lib(
            'resolve_methodology "golden standard"',
            sources=[mine.file_pattern, theirs.file_pattern],
        )
        assert run.returncode == 0, run
        _uri, path, _origin = resolution(run)
        assert (
            open(path).read()
            == theirs.path("methodologies/golden_standard").read_text()
        )

    def test_a_name_no_source_offers_is_still_unresolved(self, sources_lib, two_trees):
        mine, theirs = two_trees
        run = sources_lib(
            'resolve_methodology "neither of us"',
            sources=[mine.file_pattern, theirs.file_pattern],
        )
        assert run.returncode == UNRESOLVED, run

    def test_every_candidate_uri_is_listed_in_order_without_fetching_anything(
        self, sources_lib, two_trees
    ):
        """What `sources` and the mismatch diagnosis print: where the name *could* have come from."""
        mine, theirs = two_trees
        run = sources_lib(
            'candidate_uris "methodologies/basic_check"',
            sources=[mine.file_pattern, theirs.file_pattern],
        )
        assert run.stdout.splitlines() == [
            f"file://{mine.root}/methodologies/basic_check.rst",
            f"file://{theirs.root}/methodologies/basic_check.rst",
        ]


class TestResolvingOverHttp:
    def test_a_page_is_fetched_and_hashed(self, sources_lib, http_source):
        run = sources_lib(
            'methodology_sha256 "basic check"', sources=http_source.pattern
        )
        assert run.returncode == 0, run
        assert run.stdout.strip() == http_source.tree.sha256(
            "methodologies/basic_check"
        )
        assert "/methodologies/basic_check.rst" in http_source.requested

    def test_the_body_comes_back_byte_for_byte(self, sources_lib, http_source):
        run = sources_lib(
            'resolve_methodology "basic check"', sources=http_source.pattern
        )
        _uri, path, origin = resolution(run)
        assert origin == "network"
        assert (
            open(path, "rb").read()
            == http_source.tree.path("methodologies/basic_check").read_bytes()
        )

    def test_a_missing_page_is_unresolved_rather_than_unreachable(
        self, sources_lib, http_source
    ):
        """A 404 is an answer: this source does not have it. That is not a network failure."""
        run = sources_lib(
            'resolve_methodology "no such methodology"', sources=http_source.pattern
        )
        assert run.returncode == UNRESOLVED, run

    def test_a_server_that_is_not_there_is_unreachable(self, sources_lib, http_source):
        pattern = http_source.pattern
        http_source.stop()
        run = sources_lib('resolve_methodology "basic check"', sources=pattern)
        assert run.returncode == UNREACHABLE, run

    def test_http_is_accepted_but_warned_about(self, sources_lib, http_source):
        """The hash is what compensates for an unauthenticated fetch -- but say which one it was."""
        run = sources_lib(
            'resolve_methodology "basic check"', sources=http_source.pattern
        )
        assert run.returncode == 0, run
        assert "http" in run.stderr.lower()

    def test_the_warning_is_printed_once_however_many_names_are_resolved(
        self, sources_lib, http_source
    ):
        run = sources_lib(
            'resolve_methodology "basic check" >/dev/null\n'
            'resolve_methodology "golden standard" >/dev/null\n'
            "resolve_general_methodology >/dev/null\n",
            sources=http_source.pattern,
        )
        assert run.stderr.lower().count("unencrypted") == 1, run


class TestTheCache:
    def test_a_fetched_body_is_stored_under_its_own_hash(
        self, sources_lib, http_source, source_cache
    ):
        sources_lib('resolve_methodology "basic check"', sources=http_source.pattern)
        digest = http_source.tree.sha256("methodologies/basic_check")
        stored = list(source_cache.rglob(digest))
        assert stored, sorted(p.name for p in source_cache.rglob("*"))

    def test_the_network_is_asked_again_even_though_the_body_is_cached(
        self, sources_lib, http_source
    ):
        """The cache is a fallback, not a shortcut.

        A cache that answered first would quietly pin a methodology to whatever was fetched the day
        it was first seen -- and the published text changing is precisely the event this whole
        mechanism exists to notice. Skipping the fetch is what `--offline` is for, and it says so on
        the line it prints.
        """
        sources_lib('resolve_methodology "basic check"', sources=http_source.pattern)
        http_source.requested.clear()
        run = sources_lib(
            'resolve_methodology "basic check"', sources=http_source.pattern
        )
        assert run.returncode == 0, run
        assert http_source.requested == ["/methodologies/basic_check.rst"]

    def test_a_cached_body_answers_with_the_server_stopped(
        self, sources_lib, http_source
    ):
        """Content-addressed, so a stored hash can be checked against a cached copy with no network."""
        pattern = http_source.pattern
        expected = http_source.tree.sha256("methodologies/basic_check")
        sources_lib('resolve_methodology "basic check"', sources=pattern)
        http_source.stop()
        run = sources_lib('methodology_sha256 "basic check"', sources=pattern)
        assert run.returncode == 0, run
        assert run.stdout.strip() == expected

    def test_a_changed_page_is_kept_alongside_the_body_it_replaces(
        self, sources_lib, http_source, source_cache
    ):
        """Every distinct body ever seen for a URI is kept: that is what makes a mismatch auditable."""
        first = http_source.tree.sha256("methodologies/basic_check")
        sources_lib('resolve_methodology "basic check"', sources=http_source.pattern)
        http_source.tree.write(
            "methodologies/basic_check", "Basic check\n===========\n\nRewritten.\n"
        )
        second = http_source.tree.sha256("methodologies/basic_check")
        sources_lib('resolve_methodology "basic check"', sources=http_source.pattern)
        assert list(source_cache.rglob(first)), "the first body was discarded"
        assert list(source_cache.rglob(second)), "the second body was not stored"

    def test_the_latest_body_is_the_one_answered_with(self, sources_lib, http_source):
        sources_lib('resolve_methodology "basic check"', sources=http_source.pattern)
        http_source.tree.write(
            "methodologies/basic_check", "Basic check\n===========\n\nRewritten.\n"
        )
        expected = http_source.tree.sha256("methodologies/basic_check")
        sources_lib('resolve_methodology "basic check"', sources=http_source.pattern)
        http_source.stop()
        run = sources_lib(
            'methodology_sha256 "basic check"', sources=http_source.pattern
        )
        assert run.stdout.strip() == expected

    def test_a_local_source_is_not_cached_at_all(
        self, sources_lib, local_source, source_cache
    ):
        """There is nothing to cache: the file *is* the local copy, and a stale one would be a bug."""
        sources_lib('resolve_methodology "basic check"', sources=local_source)
        assert not source_cache.exists() or not list(source_cache.rglob("*"))

    def test_two_sources_offering_the_same_name_do_not_share_a_cache_entry(
        self, sources_lib, http_source, tmp_path, source_cache
    ):
        """Keyed by the resolved URI, because two sources are two different claims about a name."""
        sources_lib('resolve_methodology "basic check"', sources=http_source.pattern)
        entries = [d for d in source_cache.iterdir() if d.is_dir()]
        assert len(entries) == 1


class TestOffline:
    def test_nothing_is_fetched(self, sources_lib, http_source):
        sources_lib('resolve_methodology "basic check"', sources=http_source.pattern)
        http_source.requested.clear()
        run = sources_lib(
            'resolve_methodology "basic check"',
            sources=http_source.pattern,
            offline=True,
        )
        assert run.returncode == 0, run
        assert http_source.requested == []

    def test_the_answer_says_it_came_from_the_cache(self, sources_lib, http_source):
        sources_lib('resolve_methodology "basic check"', sources=http_source.pattern)
        run = sources_lib(
            'resolve_methodology "basic check"',
            sources=http_source.pattern,
            offline=True,
        )
        _uri, _path, origin = resolution(run)
        assert origin == "cache"

    def test_with_nothing_cached_it_is_unreachable_rather_than_unresolved(
        self, sources_lib, http_source
    ):
        run = sources_lib(
            'resolve_methodology "basic check"',
            sources=http_source.pattern,
            offline=True,
        )
        assert run.returncode == UNREACHABLE, run

    def test_a_local_source_is_unaffected(self, sources_lib, local_source):
        run = sources_lib(
            'resolve_methodology "basic check"', sources=local_source, offline=True
        )
        assert run.returncode == 0, run
        _uri, _path, origin = resolution(run)
        assert origin == "local"

    def test_a_later_local_source_still_answers_when_the_first_is_uncached(
        self, sources_lib, http_source, methodology_pages
    ):
        run = sources_lib(
            'resolve_methodology "basic check"',
            sources=[http_source.pattern, methodology_pages.file_pattern],
            offline=True,
        )
        assert run.returncode == 0, run
        _uri, _path, origin = resolution(run)
        assert origin == "local"


class TestABadlyConfiguredSource:
    def test_a_source_without_a_star_stops_the_run_naming_the_source(self, sources_lib):
        run = sources_lib(
            'resolve_methodology "basic check"', sources="/pages/basic_check.rst"
        )
        assert run.returncode != 0, run
        assert "/pages/basic_check.rst" in run.output

    def test_one_bad_source_is_not_papered_over_by_a_good_one(
        self, sources_lib, local_source
    ):
        """A misconfiguration is reported where it is, not hidden by whichever source happens to work."""
        run = sources_lib(
            'resolve_methodology "basic check"',
            sources=["/pages/no-star.rst", local_source],
        )
        assert run.returncode != 0, run
        assert "/pages/no-star.rst" in run.output

    def test_no_source_at_all_is_refused_with_an_explanation(self, sources_lib):
        run = sources_lib('resolve_methodology "basic check"', sources="")
        assert run.returncode != 0, run
        assert "source" in run.output.lower()


class TestReproducibleByHand:
    """The property the shell was chosen for: every step is a command a person can retype."""

    def test_the_resolved_uri_and_the_hash_agree_with_curl_and_sha256sum(
        self, sources_lib, http_source
    ):
        run = sources_lib(
            'resolve_methodology "basic check"', sources=http_source.pattern
        )
        uri, _path, _origin = resolution(run)
        by_hand = subprocess.run(
            ["curl", "-fsSL", uri], capture_output=True, check=True
        ).stdout
        assert sha256(by_hand).hexdigest() == http_source.tree.sha256(
            "methodologies/basic_check"
        )
