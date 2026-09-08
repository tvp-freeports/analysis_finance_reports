"""The document flows, now that a methodology's text comes from a source rather than from the tool.

`create-document`, `grant`, `ungrant`, `update` and `check-grants` used to hash a file that shipped
inside the package: the meaning of a grant travelled with the installed version, and upgrading the
command changed what everyone's documents said. It does not any more -- every hash in a document is
the hash of a text resolved from a *configured source* -- and these tests are about that seam.

They run the real command against a real (throwaway) keyring, because what is being checked is a
signature over a document, and there is no way to check that without one.
"""

import pytest


class TestCreatingADocument:
    def test_the_version_is_the_hash_of_the_resolved_general_methodology(
        self, run_validate, tmp_repo, signer, local_source, methodology_pages
    ):
        run = run_validate(
            "create-document",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode == 0, run
        document = tmp_repo.document(signer).read_text()
        assert methodology_pages.sha256("general_methodology") in document

    def test_a_different_source_gives_a_different_version(
        self, run_validate, tmp_repo, signer, methodology_pages, tmp_path
    ):
        """The point of the whole change: the text is the contract, and the source names the text."""
        from conftest import PageTree

        mine = PageTree(root=tmp_path / "mine")
        mine.write(
            "general_methodology", "General methodology\n===================\n\nMine.\n"
        )
        run_validate(
            "create-document",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=mine.file_pattern,
        )
        document = tmp_repo.document(signer).read_text()
        assert mine.sha256("general_methodology") in document
        assert methodology_pages.sha256("general_methodology") not in document

    def test_an_unresolvable_general_methodology_stops_the_run(
        self, run_validate, tmp_repo, signer, tmp_path
    ):
        """A document whose `version` is empty is not a document; refusing beats writing one."""
        empty = tmp_path / "empty-source"
        empty.mkdir()
        run = run_validate(
            "create-document",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=f"file://{empty}/*.rst",
        )
        assert run.returncode != 0, run
        assert not tmp_repo.document(signer).exists()


class TestAdoptingAMethodology:
    def test_a_methodology_is_adopted_at_the_hash_the_source_gives(
        self,
        run_validate,
        tmp_repo,
        signer,
        local_source,
        methodology_pages,
        signed_document,
    ):
        run = run_validate(
            "grant",
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode == 0, run
        assert (
            methodology_pages.sha256("methodologies/basic_check")
            in signed_document.read_text()
        )

    def test_a_methodology_no_source_offers_is_refused_by_name(
        self, run_validate, tmp_repo, signer, local_source, signed_document
    ):
        run = run_validate(
            "grant",
            "with",
            "no such methodology",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode != 0, run
        assert "no such methodology" in run.output


class TestGrantingAFile:
    @pytest.fixture
    def adopted(self, run_validate, tmp_repo, signer, local_source, signed_document):
        run = run_validate(
            "grant",
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode == 0, run
        return signed_document

    def test_a_file_is_granted_at_its_own_hash(
        self, run_validate, tmp_repo, signer, local_source, adopted
    ):
        target = tmp_repo.root / "tests/formats/FOO-EN24/1/out/funds.csv"
        run = run_validate(
            "grant",
            str(target),
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
            cwd=tmp_repo.root,
        )
        assert run.returncode == 0, run
        document = adopted.read_text()
        assert "tests/formats/FOO-EN24/1/out/funds.csv" in document
        assert tmp_repo.sha256("tests/formats/FOO-EN24/1/out/funds.csv") in document

    def test_the_document_still_checks_out_afterwards(
        self, run_validate, tmp_repo, signer, local_source, adopted
    ):
        target = tmp_repo.root / "tests/formats/FOO-EN24/1/out/funds.csv"
        run_validate(
            "grant",
            str(target),
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
            cwd=tmp_repo.root,
        )
        run = run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode == 0, run

    def test_changing_the_file_afterwards_is_reported(
        self, run_validate, tmp_repo, signer, local_source, adopted
    ):
        target = tmp_repo.root / "tests/formats/FOO-EN24/1/out/funds.csv"
        run_validate(
            "grant",
            str(target),
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
            cwd=tmp_repo.root,
        )
        tmp_repo.write(
            "tests/formats/FOO-EN24/1/out/funds.csv", "isin,name\nIT0002,Beta\n"
        )
        run = run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode != 0, run
        assert "hash mismatch" in run.output.lower()


class TestWhatAChangedPageDoes:
    """The mechanism working: a published text changing invalidates the grants made under it."""

    @pytest.fixture
    def adopted(self, run_validate, tmp_repo, signer, local_source, signed_document):
        run_validate(
            "grant",
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        return signed_document

    def test_editing_the_page_makes_the_check_fail(
        self, run_validate, tmp_repo, signer, local_source, methodology_pages, adopted
    ):
        methodology_pages.write(
            "methodologies/basic_check",
            "Basic check\n===========\n\nRewritten by its author.\n",
        )
        run = run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode != 0, run
        assert "methodology" in run.output.lower()

    def test_the_explanation_names_a_differently_configured_source_first(
        self, run_validate, tmp_repo, signer, local_source, methodology_pages, adopted
    ):
        """The price of not recording the source in the document, paid in prose.

        The document stores a name and a hash and deliberately not the source, because the source is
        a contract between the granter and the verifier. That makes a mismatch ambiguous, and the
        likelier of the two readings -- you and the granter configured different sources -- has to
        come first, or every such run sends someone hunting for a change that never happened.
        """
        methodology_pages.write(
            "methodologies/basic_check", "Basic check\n===========\n\nRewritten.\n"
        )
        run = run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        explanation = run.output
        assert "source" in explanation.lower()
        assert local_source in explanation

    def test_the_explanation_shows_the_uri_the_name_resolved_to(
        self, run_validate, tmp_repo, signer, local_source, methodology_pages, adopted
    ):
        methodology_pages.write(
            "methodologies/basic_check", "Basic check\n===========\n\nRewritten.\n"
        )
        run = run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert str(methodology_pages.path("methodologies/basic_check")) in run.output

    def test_update_methodology_restates_the_adoption(
        self, run_validate, tmp_repo, signer, local_source, methodology_pages, adopted
    ):
        methodology_pages.write(
            "methodologies/basic_check", "Basic check\n===========\n\nRewritten.\n"
        )
        run = run_validate(
            "update",
            "methodology",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode == 0, run
        assert (
            methodology_pages.sha256("methodologies/basic_check") in adopted.read_text()
        )

    def test_update_version_restates_the_general_methodology(
        self,
        run_validate,
        tmp_repo,
        signer,
        local_source,
        methodology_pages,
        signed_document,
    ):
        methodology_pages.write(
            "general_methodology",
            "General methodology\n===================\n\nRewritten.\n",
        )
        run = run_validate(
            "update",
            "version",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode == 0, run
        assert (
            methodology_pages.sha256("general_methodology")
            in signed_document.read_text()
        )


class TestTheNewDiagnoses:
    """Two failures that used to be impossible, because the pages could not be missing."""

    @pytest.fixture
    def adopted_over_http(self, run_validate, tmp_repo, signer, http_source):
        run = run_validate(
            "create-document",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=http_source.pattern,
        )
        assert run.returncode == 0, run
        run = run_validate(
            "sign-document",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=http_source.pattern,
        )
        assert run.returncode == 0, run
        run = run_validate(
            "grant",
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=http_source.pattern,
        )
        assert run.returncode == 0, run
        return tmp_repo.document(signer)

    def test_a_methodology_no_source_offers_is_reported_as_unresolved(
        self,
        run_validate,
        tmp_repo,
        signer,
        local_source,
        methodology_pages,
        signed_document,
    ):
        run_validate(
            "grant",
            "with",
            "basic check",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        methodology_pages.path("methodologies/basic_check").unlink()
        run = run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode != 0, run
        assert "no configured source offers it" in run.output.lower()

    def test_an_unreachable_source_is_neither_a_match_nor_a_mismatch(
        self, run_validate, tmp_repo, signer, http_source, adopted_over_http
    ):
        """`--offline` with nothing cached is the honest "I cannot tell", and it says so."""
        pattern = http_source.pattern
        http_source.stop()
        run = run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=pattern,
            env={"XDG_CACHE_HOME": "/nonexistent-cache-for-this-test"},
        )
        assert "could not be reached" in run.output.lower()
        assert "mismatch" not in run.output.lower()

    def test_and_it_does_not_pass_either(
        self, run_validate, tmp_repo, signer, http_source, adopted_over_http
    ):
        """The third status, and the reason it exists.

        Not counting an unmade check as an error is right; reporting the run as a pass because
        the count stayed at zero is not. A grant nobody compared against anything has not been
        verified, and saying so is the whole job.
        """
        pattern = http_source.pattern
        http_source.stop()
        run = run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=pattern,
            env={"XDG_CACHE_HOME": "/nonexistent-cache-for-this-test"},
        )
        assert run.returncode == 3, run
        assert "passed verification" not in run.output.lower()

    def test_and_it_says_the_grants_are_not_vouched_for(
        self, run_validate, tmp_repo, signer, http_source, adopted_over_http
    ):
        """An exit status nobody reads is not an answer: the words have to be there too."""
        pattern = http_source.pattern
        http_source.stop()
        run = run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=pattern,
            env={"XDG_CACHE_HOME": "/nonexistent-cache-for-this-test"},
        )
        assert "not vouched for" in run.output.lower(), run
        assert "ungranted" in run.output.lower(), run

    def test_offline_answers_from_the_cache_and_says_that_is_what_it_did(
        self, run_validate, tmp_repo, signer, http_source, adopted_over_http
    ):
        pattern = http_source.pattern
        http_source.stop()
        run = run_validate(
            "check-grants",
            "--offline",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=pattern,
        )
        assert run.returncode == 0, run
        assert "cache" in run.output.lower()
        # Answered, not skipped: `--offline` that found what it needed is a conclusive run, and
        # must not be tarred with the status of one that found nothing.
        assert "passed verification" in run.output.lower(), run


class TestTheShippedPagesAreGone:
    def test_the_package_carries_no_methodology_pages(self):
        """The duplication is gone with them: `docs/validation/` inside the package was a
        byte-identical copy of `docs/source/validation/` in this repository, kept in step by luck.
        """
        from pathlib import Path

        from freeports_validate import cli

        assert not (Path(cli.__file__).parent / "docs").exists()

    def test_the_package_data_no_longer_claims_to_ship_them(self):
        import tomllib
        from pathlib import Path

        from freeports_validate import cli

        pyproject = Path(cli.__file__).parents[2] / "pyproject.toml"
        declared = tomllib.loads(pyproject.read_text())["tool"]["setuptools"][
            "package-data"
        ]["freeports_validate"]
        assert not [entry for entry in declared if entry.startswith("docs")]
