"""`freeports-validate sources`: the answer to "why does my hash differ from yours".

A validation document records a methodology's name and its hash, and deliberately not the source it
came from -- the source is a contract each party writes in their own configuration. That makes a
hash mismatch ambiguous by construction, and this subcommand is what resolves the ambiguity: it
prints the sources in use, in priority order, and what each name actually resolves to here. Two
people comparing their output can see in one line whether they are reading the same publication.
"""

import pytest


class TestListingTheSources:
    def test_the_configured_sources_are_printed_in_priority_order(
        self, run_validate, tmp_repo, local_source, methodology_pages, tmp_path
    ):
        from conftest import PageTree

        mine = PageTree(root=tmp_path / "mine")
        mine.write("general_methodology", "General methodology\n===================\n")
        run = run_validate(
            "sources", "-s", mine.file_pattern, "-s", local_source, repo=tmp_repo
        )
        assert run.returncode == 0, run
        assert run.output.index(mine.file_pattern) < run.output.index(local_source), run

    def test_it_needs_no_signing_key(self, run_validate, tmp_repo, local_source):
        """Reading what your own configuration resolves is not an act done in anybody's name."""
        run = run_validate("sources", repo=tmp_repo, sources=local_source)
        assert run.returncode == 0, run

    def test_a_source_without_a_star_is_refused_by_name(self, run_validate, tmp_repo):
        run = run_validate("sources", repo=tmp_repo, sources="/pages/basic_check.rst")
        assert run.returncode != 0, run
        assert "/pages/basic_check.rst" in run.output


class TestWhatEachNameResolvesTo:
    def test_the_general_methodology_is_always_reported(
        self, run_validate, tmp_repo, local_source, methodology_pages
    ):
        """Every claim in every document is made under it, so it is never not worth showing."""
        run = run_validate("sources", repo=tmp_repo, sources=local_source)
        assert "general_methodology" in run.output
        assert methodology_pages.sha256("general_methodology") in run.output

    def test_a_name_given_as_an_argument_is_reported(
        self, run_validate, tmp_repo, local_source, methodology_pages
    ):
        run = run_validate(
            "sources", "basic check", repo=tmp_repo, sources=local_source
        )
        assert methodology_pages.sha256("methodologies/basic_check") in run.output

    def test_the_resolved_uri_is_shown_next_to_the_hash(
        self, run_validate, tmp_repo, local_source, methodology_pages
    ):
        run = run_validate(
            "sources", "basic check", repo=tmp_repo, sources=local_source
        )
        assert str(methodology_pages.path("methodologies/basic_check")) in run.output

    def test_a_name_no_source_offers_is_said_so_without_failing_the_run(
        self, run_validate, tmp_repo, local_source
    ):
        """This subcommand reports a configuration; it does not judge it."""
        run = run_validate(
            "sources", "no such methodology", repo=tmp_repo, sources=local_source
        )
        assert run.returncode == 0, run
        assert "no such methodology" in run.output


class TestShadowing:
    """A methodology two sources both offer is resolved from the first, and that must be visible."""

    @pytest.fixture
    def two_trees(self, tmp_path, methodology_pages):
        from conftest import PageTree

        mine = PageTree(root=tmp_path / "mine")
        mine.write(
            "general_methodology", "General methodology\n===================\n\nMine.\n"
        )
        mine.write(
            "methodologies/basic_check", "Basic check\n===========\n\nMy own text.\n"
        )
        return mine, methodology_pages

    def test_the_winning_source_is_the_one_reported(
        self, run_validate, tmp_repo, two_trees
    ):
        mine, theirs = two_trees
        run = run_validate(
            "sources",
            "basic check",
            "-s",
            mine.file_pattern,
            "-s",
            theirs.file_pattern,
            repo=tmp_repo,
        )
        assert mine.sha256("methodologies/basic_check") in run.output
        assert theirs.sha256("methodologies/basic_check") not in run.output

    def test_a_shadowed_source_is_named_rather_than_left_mysterious(
        self, run_validate, tmp_repo, two_trees
    ):
        """The failure this prevents: a page you edited having no effect, with nothing saying why."""
        mine, theirs = two_trees
        run = run_validate(
            "sources",
            "basic check",
            "-s",
            mine.file_pattern,
            "-s",
            theirs.file_pattern,
            repo=tmp_repo,
        )
        assert "shadow" in run.output.lower()
        assert str(theirs.path("methodologies/basic_check")) in run.output


class TestTheRepositorysOwnMethodologies:
    def test_every_methodology_adopted_in_the_repository_is_reported(
        self,
        run_validate,
        tmp_repo,
        signer,
        local_source,
        methodology_pages,
        signed_document,
    ):
        """The set that actually matters: what this repository's documents were written under."""
        run_validate(
            "grant",
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        run = run_validate("sources", repo=tmp_repo, sources=local_source)
        assert "basic check" in run.output
        assert methodology_pages.sha256("methodologies/basic_check") in run.output

    def test_it_works_in_a_repository_with_no_documents_at_all(
        self, run_validate, tmp_repo, local_source
    ):
        run = run_validate("sources", repo=tmp_repo, sources=local_source)
        assert run.returncode == 0, run


class TestOverHttp:
    def test_a_remote_source_is_resolved_and_hashed(
        self, run_validate, tmp_repo, http_source
    ):
        run = run_validate(
            "sources", "basic check", repo=tmp_repo, sources=http_source.pattern
        )
        assert run.returncode == 0, run
        assert http_source.tree.sha256("methodologies/basic_check") in run.output
        assert http_source.url("methodologies/basic_check") in run.output

    def test_an_unreachable_source_is_reported_as_such_rather_than_omitted(
        self, run_validate, tmp_repo, http_source
    ):
        pattern = http_source.pattern
        http_source.stop()
        run = run_validate("sources", "basic check", repo=tmp_repo, sources=pattern)
        assert "could not be reached" in run.output.lower()
