"""Sub-hashes: pinning the links and images a methodology page depends on.

A methodology page is prose, and prose cites things -- a diagram, a specification, another document.
A grant made under that page is, in part, a claim about what those things said, so the page can pin
each of them by hash in an ordinary reStructuredText comment::

    .. image:: assets/pipeline.svg

    .. sha256: assets/pipeline.svg 3f2a1b...c9

`..` followed by text that is not a directive is a comment in every reStructuredText parser: it
renders as nothing, breaks no build, and reads plainly in the source.

**The pins are not a second trust root.** They are lines of the page, so the page's own `sha256` --
the one a validation document records -- already commits to every one of them. Verifying them is
therefore an optional deepening (`--deep`), never a separate thing to trust, and a page whose pins
were never checked is not thereby less trustworthy than its hash says.

Three commands are involved: `check-methodology` inspects one page, `check-grants --deep` does it
for every adopted methodology, and `refresh-links` rewrites a *local* page's pins -- loudly, because
doing so changes the page's own hash and invalidates every grant made under it.
"""

import json
import re

import pytest


PIPELINE_SVG = '<svg xmlns="http://www.w3.org/2000/svg"><title>pipeline</title></svg>\n'
SPECIFICATION = "The specification this methodology cites.\n"

UNREACHABLE_URI = "https://unreachable.invalid/specification.txt"


def page_citing(*, image=None, link=None, pins=()):
    """A methodology page citing an image and a link, with the pins it declares for them."""
    body = [
        "Basic check",
        "===========",
        "",
        "The lightest of the methodologies.",
        "",
    ]
    if image is not None:
        body += [f".. image:: {image}", ""]
    if link is not None:
        body += [f"See the `specification <{link}>`_ for the derivation.", ""]
    for target, digest in pins:
        body += [f".. sha256: {target} {digest}", ""]
    return "\n".join(body) + "\n"


# ---------------------------------------------------------------------------
# 3.1 -- reading the pins and the targets
# ---------------------------------------------------------------------------


def links(rst_paths, text):
    return rst_paths.links(text)


class TestFindingTheTargets:
    def test_an_image_directive_is_a_target(self, rst_paths):
        found = links(rst_paths, page_citing(image="assets/pipeline.svg"))
        assert found["unpinned"] == ["assets/pipeline.svg"]

    def test_a_figure_directive_is_a_target(self, rst_paths):
        page = "Title\n=====\n\n.. figure:: assets/diagram.png\n\n   A caption.\n"
        assert links(rst_paths, page)["unpinned"] == ["assets/diagram.png"]

    def test_an_inline_link_is_a_target(self, rst_paths):
        found = links(rst_paths, page_citing(link="https://example.invalid/spec.pdf"))
        assert found["unpinned"] == ["https://example.invalid/spec.pdf"]

    def test_a_hyperlink_target_definition_is_a_target(self, rst_paths):
        page = "Title\n=====\n\n.. _the spec: https://example.invalid/spec.pdf\n"
        assert links(rst_paths, page)["unpinned"] == [
            "https://example.invalid/spec.pdf"
        ]

    def test_a_bare_uri_in_prose_is_a_target(self, rst_paths):
        """docutils turns one into a link, so a page that says it has none would be lying."""
        page = (
            "Title\n=====\n\nSee https://example.invalid/spec.pdf for the derivation.\n"
        )
        assert links(rst_paths, page)["unpinned"] == [
            "https://example.invalid/spec.pdf"
        ]

    def test_a_sentence_ending_stop_is_not_part_of_the_uri(self, rst_paths):
        page = "Title\n=====\n\nSee https://example.invalid/spec.pdf.\n"
        assert links(rst_paths, page)["unpinned"] == [
            "https://example.invalid/spec.pdf"
        ]

    def test_the_same_target_twice_is_one_target(self, rst_paths):
        page = (
            "Title\n=====\n\n"
            "See https://example.invalid/spec.pdf and, again,\n"
            "`the spec <https://example.invalid/spec.pdf>`_.\n"
        )
        assert links(rst_paths, page)["unpinned"] == [
            "https://example.invalid/spec.pdf"
        ]

    def test_targets_come_back_in_the_order_the_page_cites_them(self, rst_paths):
        page = page_citing(image="assets/a.svg", link="https://example.invalid/b.pdf")
        assert links(rst_paths, page)["unpinned"] == [
            "assets/a.svg",
            "https://example.invalid/b.pdf",
        ]

    def test_a_page_citing_nothing_has_no_targets(self, rst_paths):
        assert links(rst_paths, page_citing())["unpinned"] == []


class TestWhatIsNotATarget:
    """A page's prose cites things; its examples only look as though they do."""

    def test_a_uri_inside_a_literal_block_is_not_a_target(self, rst_paths):
        page = (
            "Title\n=====\n\nRun it like this::\n\n"
            "    curl https://example.invalid/spec.pdf\n\nand then stop.\n"
        )
        assert links(rst_paths, page)["unpinned"] == []

    def test_a_uri_inside_a_code_block_is_not_a_target(self, rst_paths):
        page = (
            "Title\n=====\n\n.. code-block:: console\n\n"
            "    $ curl https://example.invalid/spec.pdf\n"
        )
        assert links(rst_paths, page)["unpinned"] == []

    def test_a_uri_inside_an_admonition_is_still_a_target(self, rst_paths):
        """A `note` reads as prose and its links are real links; only literal blocks are examples."""
        page = (
            "Title\n=====\n\n.. note::\n\n"
            "    See https://example.invalid/spec.pdf for the derivation.\n"
        )
        assert links(rst_paths, page)["unpinned"] == [
            "https://example.invalid/spec.pdf"
        ]

    def test_a_uri_named_only_by_its_own_pin_is_not_a_target(self, rst_paths):
        """Otherwise a stale pin would keep itself alive by being the only mention left."""
        page = page_citing(pins=[("https://example.invalid/gone.pdf", "a" * 64)])
        assert links(rst_paths, page)["unpinned"] == []


class TestPinnedAgainstUnpinned:
    DIGEST = "b" * 64

    def test_a_pinned_target_carries_its_hash(self, rst_paths):
        page = page_citing(
            image="assets/pipeline.svg", pins=[("assets/pipeline.svg", self.DIGEST)]
        )
        assert links(rst_paths, page)["pinned"] == [
            {"target": "assets/pipeline.svg", "sha256": self.DIGEST}
        ]

    def test_a_pinned_target_is_not_also_unpinned(self, rst_paths):
        page = page_citing(
            image="assets/pipeline.svg", pins=[("assets/pipeline.svg", self.DIGEST)]
        )
        assert links(rst_paths, page)["unpinned"] == []

    def test_the_two_are_reported_side_by_side(self, rst_paths):
        page = page_citing(
            image="assets/pipeline.svg",
            link="https://example.invalid/spec.pdf",
            pins=[("assets/pipeline.svg", self.DIGEST)],
        )
        found = links(rst_paths, page)
        assert [entry["target"] for entry in found["pinned"]] == ["assets/pipeline.svg"]
        assert found["unpinned"] == ["https://example.invalid/spec.pdf"]

    def test_a_pin_for_something_the_page_no_longer_cites_is_stale(self, rst_paths):
        page = page_citing(
            image="assets/pipeline.svg",
            pins=[
                ("assets/pipeline.svg", self.DIGEST),
                ("assets/removed.svg", "c" * 64),
            ],
        )
        found = links(rst_paths, page)
        assert [entry["target"] for entry in found["stale"]] == ["assets/removed.svg"]

    def test_a_pin_is_recognised_wherever_it_sits(self, rst_paths):
        """The comment names its own target, so being next to it is for the reader, not the tool."""
        page = (
            "Title\n=====\n\n"
            f".. sha256: assets/pipeline.svg {self.DIGEST}\n\n"
            ".. image:: assets/pipeline.svg\n"
        )
        assert [entry["target"] for entry in links(rst_paths, page)["pinned"]] == [
            "assets/pipeline.svg"
        ]

    def test_a_comment_that_is_not_a_pin_is_left_alone(self, rst_paths):
        page = page_citing(image="assets/pipeline.svg") + ".. a plain comment\n"
        assert links(rst_paths, page)["pinned"] == []

    def test_a_pin_with_a_hash_of_the_wrong_length_is_not_a_pin(self, rst_paths):
        """Silently reading a truncated hash as a pin would compare against a claim nobody made."""
        page = page_citing(
            image="assets/pipeline.svg", pins=[("assets/pipeline.svg", "abc123")]
        )
        found = links(rst_paths, page)
        assert found["pinned"] == []
        assert found["unpinned"] == ["assets/pipeline.svg"]


class TestTheParserStillReportsBothThings:
    def test_one_call_answers_about_paths_and_links_together(self, rst_paths_script):
        run = rst_paths_script(stdin=page_citing(image="assets/pipeline.svg"))
        assert run.returncode == 0, run
        parsed = json.loads(run.stdout)
        assert "supported_paths" in parsed
        assert parsed["links"]["unpinned"] == ["assets/pipeline.svg"]


# ---------------------------------------------------------------------------
# 3.3 -- rewriting the pins
# ---------------------------------------------------------------------------


class TestRewritingAPage:
    DIGEST = "d" * 64
    OTHER = "e" * 64

    def pin(self, rst_paths, page, hashes):
        return rst_paths.pin(page, hashes)

    def test_a_missing_pin_is_written(self, rst_paths):
        page = page_citing(image="assets/pipeline.svg")
        rewritten = self.pin(rst_paths, page, {"assets/pipeline.svg": self.DIGEST})
        assert f".. sha256: assets/pipeline.svg {self.DIGEST}" in rewritten

    def test_the_pin_sits_next_to_what_it_pins(self, rst_paths):
        page = page_citing(image="assets/pipeline.svg")
        rewritten = self.pin(rst_paths, page, {"assets/pipeline.svg": self.DIGEST})
        lines = rewritten.splitlines()
        directive = lines.index(".. image:: assets/pipeline.svg")
        comment = lines.index(f".. sha256: assets/pipeline.svg {self.DIGEST}")
        assert 0 < comment - directive <= 2, rewritten

    def test_the_rewritten_page_still_parses_the_same_way(self, rst_paths):
        page = page_citing(image="assets/pipeline.svg")
        rewritten = self.pin(rst_paths, page, {"assets/pipeline.svg": self.DIGEST})
        assert links(rst_paths, rewritten)["pinned"] == [
            {"target": "assets/pipeline.svg", "sha256": self.DIGEST}
        ]

    def test_an_existing_pin_is_updated_in_place(self, rst_paths):
        page = page_citing(
            image="assets/pipeline.svg", pins=[("assets/pipeline.svg", self.OTHER)]
        )
        rewritten = self.pin(rst_paths, page, {"assets/pipeline.svg": self.DIGEST})
        assert self.OTHER not in rewritten
        assert rewritten.count(".. sha256: assets/pipeline.svg") == 1

    def test_rewriting_twice_changes_nothing_the_second_time(self, rst_paths):
        page = page_citing(
            image="assets/pipeline.svg", link="https://example.invalid/spec.pdf"
        )
        hashes = {
            "assets/pipeline.svg": self.DIGEST,
            "https://example.invalid/spec.pdf": self.OTHER,
        }
        once = self.pin(rst_paths, page, hashes)
        assert self.pin(rst_paths, once, hashes) == once

    def test_a_stale_pin_is_removed(self, rst_paths):
        page = page_citing(
            image="assets/pipeline.svg",
            pins=[
                ("assets/pipeline.svg", self.DIGEST),
                ("assets/removed.svg", self.OTHER),
            ],
        )
        rewritten = self.pin(rst_paths, page, {"assets/pipeline.svg": self.DIGEST})
        assert "assets/removed.svg" not in rewritten

    def test_a_pin_whose_target_could_not_be_fetched_is_kept(self, rst_paths):
        """Unpinning something because the network failed would be a silent loss of a claim."""
        page = page_citing(
            image="assets/pipeline.svg", pins=[("assets/pipeline.svg", self.DIGEST)]
        )
        rewritten = self.pin(rst_paths, page, {})
        assert f".. sha256: assets/pipeline.svg {self.DIGEST}" in rewritten

    def test_the_prose_is_not_disturbed(self, rst_paths):
        page = page_citing(image="assets/pipeline.svg")
        rewritten = self.pin(rst_paths, page, {"assets/pipeline.svg": self.DIGEST})
        for line in page.splitlines():
            if line.strip():
                assert line in rewritten.splitlines(), line


# ---------------------------------------------------------------------------
# The shell seam: resolving a target against the page that cites it
# ---------------------------------------------------------------------------


@pytest.fixture
def cited(methodology_pages):
    """A `basic check` page citing a local image, with the image beside it."""
    (methodology_pages.root / "methodologies" / "assets").mkdir(
        parents=True, exist_ok=True
    )
    (methodology_pages.root / "methodologies" / "assets" / "pipeline.svg").write_text(
        PIPELINE_SVG
    )
    return methodology_pages


class TestResolvingATarget:
    def test_an_absolute_uri_stands_for_itself(self, links_lib, local_source):
        run = links_lib(
            'link_target_uri "https://example.invalid/a.pdf" "file:///pages/b.rst"',
            sources=local_source,
        )
        assert run.stdout.strip() == "https://example.invalid/a.pdf"

    def test_a_relative_target_is_resolved_against_the_page_that_cites_it(
        self, links_lib, local_source
    ):
        """A page's own directory, not the working directory: the page is what the path is in."""
        run = links_lib(
            'link_target_uri "assets/a.svg" "file:///pages/methodologies/b.rst"',
            sources=local_source,
        )
        assert run.stdout.strip() == "file:///pages/methodologies/assets/a.svg"

    def test_a_relative_target_of_a_remote_page_stays_remote(
        self, links_lib, local_source
    ):
        run = links_lib(
            'link_target_uri "assets/a.svg" "https://docs.invalid/validation/b.rst"',
            sources=local_source,
        )
        assert run.stdout.strip() == "https://docs.invalid/validation/assets/a.svg"


# ---------------------------------------------------------------------------
# 3.2 -- `check-methodology`
# ---------------------------------------------------------------------------


@pytest.fixture
def pinned_page(cited, rst_paths):
    """The cited page, with its image pinned at the image's real hash."""
    from hashlib import sha256

    digest = sha256(PIPELINE_SVG.encode()).hexdigest()
    cited.write(
        "methodologies/basic_check",
        page_citing(
            image="assets/pipeline.svg", pins=[("assets/pipeline.svg", digest)]
        ),
    )
    return cited


class TestCheckMethodology:
    def test_it_reports_the_page_and_its_hash(
        self, run_validate, tmp_repo, local_source, pinned_page
    ):
        run = run_validate(
            "check-methodology", "basic check", repo=tmp_repo, sources=local_source
        )
        assert run.returncode == 0, run
        assert pinned_page.sha256("methodologies/basic_check") in run.output, run

    def test_it_needs_no_signing_key(
        self, run_validate, tmp_repo, local_source, pinned_page
    ):
        """Reading somebody else's methodology is not an act done in anybody's name."""
        run = run_validate(
            "check-methodology", "basic check", repo=tmp_repo, sources=local_source
        )
        assert run.returncode == 0, run

    def test_it_counts_the_pinned_and_the_unpinned(
        self, run_validate, tmp_repo, local_source, cited
    ):
        cited.write(
            "methodologies/basic_check",
            page_citing(image="assets/pipeline.svg", link=UNREACHABLE_URI),
        )
        run = run_validate(
            "check-methodology", "basic check", repo=tmp_repo, sources=local_source
        )
        assert run.returncode == 0, run
        assert "2" in run.output, run

    def test_without_deep_nothing_is_fetched(
        self, run_validate, tmp_repo, local_source, cited
    ):
        """An unreachable target must not fail a shallow check: it was never going to be asked."""
        cited.write("methodologies/basic_check", page_citing(link=UNREACHABLE_URI))
        run = run_validate(
            "check-methodology", "basic check", repo=tmp_repo, sources=local_source
        )
        assert run.returncode == 0, run

    def test_deep_confirms_a_matching_target(
        self, run_validate, tmp_repo, local_source, pinned_page
    ):
        run = run_validate(
            "check-methodology",
            "--deep",
            "basic check",
            repo=tmp_repo,
            sources=local_source,
        )
        assert run.returncode == 0, run
        assert "assets/pipeline.svg" in run.output, run

    def test_deep_catches_a_mutated_target(
        self, run_validate, tmp_repo, local_source, pinned_page
    ):
        (pinned_page.root / "methodologies" / "assets" / "pipeline.svg").write_text(
            "<svg><title>something else</title></svg>\n"
        )
        run = run_validate(
            "check-methodology",
            "--deep",
            "basic check",
            repo=tmp_repo,
            sources=local_source,
        )
        assert run.returncode != 0, run
        assert "assets/pipeline.svg" in run.output, run

    def test_the_mismatch_is_explained(
        self, run_validate, tmp_repo, local_source, pinned_page
    ):
        (pinned_page.root / "methodologies" / "assets" / "pipeline.svg").write_text(
            "<svg><title>something else</title></svg>\n"
        )
        run = run_validate(
            "check-methodology",
            "--deep",
            "basic check",
            repo=tmp_repo,
            sources=local_source,
        )
        assert "What these errors mean" in run.output, run

    def test_an_unreachable_target_does_not_fail_the_run(
        self, run_validate, tmp_repo, local_source, cited
    ):
        """Neither a match nor a mismatch: the same three-valued answer the resolver gives."""
        cited.write(
            "methodologies/basic_check",
            page_citing(link=UNREACHABLE_URI, pins=[(UNREACHABLE_URI, "f" * 64)]),
        )
        run = run_validate(
            "check-methodology",
            "--deep",
            "basic check",
            repo=tmp_repo,
            sources=local_source,
            env={"FREEPORTS_VALIDATE_OFFLINE": "1"},
        )
        assert run.returncode == 0, run

    def test_a_methodology_no_source_offers_is_refused(
        self, run_validate, tmp_repo, local_source
    ):
        run = run_validate(
            "check-methodology",
            "no such methodology",
            repo=tmp_repo,
            sources=local_source,
        )
        assert run.returncode != 0, run


class TestCheckGrantsDeep:
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

    def test_a_changed_pinned_target_is_noticed_without_being_asked(
        self, run_validate, tmp_repo, signer, local_source, pinned_page, adopted
    ):
        """Following the pins is the default: it is the question a reader is really asking."""
        (pinned_page.root / "methodologies" / "assets" / "pipeline.svg").write_text(
            "<svg><title>something else</title></svg>\n"
        )
        run = run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode != 0, run

    def test_no_deep_stops_the_pins_being_checked(
        self, run_validate, tmp_repo, signer, local_source, pinned_page, adopted
    ):
        """The way out, for when the fetch per pinned resource is what you cannot afford.

        The page's own hash still matches, and that is what the document recorded: turning the
        transitive check off is a decision about cost, not a claim that the page is untrustworthy.
        """
        (pinned_page.root / "methodologies" / "assets" / "pipeline.svg").write_text(
            "<svg><title>something else</title></svg>\n"
        )
        run = run_validate(
            "check-grants",
            "--no-deep",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode == 0, run

    def test_deep_checks_every_adopted_methodology(
        self, run_validate, tmp_repo, signer, local_source, pinned_page, adopted
    ):
        (pinned_page.root / "methodologies" / "assets" / "pipeline.svg").write_text(
            "<svg><title>something else</title></svg>\n"
        )
        run = run_validate(
            "check-grants",
            "--deep",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode != 0, run
        assert "assets/pipeline.svg" in run.output, run

    def test_deep_passes_when_every_pin_holds(
        self, run_validate, tmp_repo, signer, local_source, pinned_page, adopted
    ):
        run = run_validate(
            "check-grants",
            "--deep",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode == 0, run


# ---------------------------------------------------------------------------
# 3.3 -- `refresh-links`
# ---------------------------------------------------------------------------


class TestRefreshLinks:
    def test_it_writes_the_pins_of_a_local_page(
        self, run_validate, tmp_repo, local_source, cited
    ):
        from hashlib import sha256

        page = cited.path("methodologies/basic_check")
        page.write_text(page_citing(image="assets/pipeline.svg"))
        run = run_validate(
            "refresh-links", str(page), repo=tmp_repo, sources=local_source
        )
        assert run.returncode == 0, run
        assert sha256(PIPELINE_SVG.encode()).hexdigest() in page.read_text()

    def test_running_it_twice_changes_nothing(
        self, run_validate, tmp_repo, local_source, cited
    ):
        page = cited.path("methodologies/basic_check")
        page.write_text(page_citing(image="assets/pipeline.svg"))
        run_validate("refresh-links", str(page), repo=tmp_repo, sources=local_source)
        once = page.read_text()
        run_validate("refresh-links", str(page), repo=tmp_repo, sources=local_source)
        assert page.read_text() == once

    def test_it_says_the_page_hash_changed(
        self, run_validate, tmp_repo, local_source, cited
    ):
        """The whole point of it being a separate command: every grant under this page is now
        invalid, and that is the mechanism working rather than a bookkeeping nuisance."""
        page = cited.path("methodologies/basic_check")
        page.write_text(page_citing(image="assets/pipeline.svg"))
        run = run_validate(
            "refresh-links", str(page), repo=tmp_repo, sources=local_source
        )
        assert "invalid" in run.output.lower(), run

    def test_it_reports_both_hashes(self, run_validate, tmp_repo, local_source, cited):
        page = cited.path("methodologies/basic_check")
        before = page_citing(image="assets/pipeline.svg")
        page.write_text(before)
        run = run_validate(
            "refresh-links", str(page), repo=tmp_repo, sources=local_source
        )
        hashes = set(re.findall(r"\b[0-9a-f]{64}\b", run.output))
        assert len(hashes) >= 2, run

    def test_a_page_that_cites_nothing_is_left_alone(
        self, run_validate, tmp_repo, local_source, cited
    ):
        page = cited.path("methodologies/basic_check")
        page.write_text(page_citing())
        before = page.read_text()
        run = run_validate(
            "refresh-links", str(page), repo=tmp_repo, sources=local_source
        )
        assert run.returncode == 0, run
        assert page.read_text() == before

    def test_a_remote_page_is_refused(self, run_validate, tmp_repo, local_source):
        """It rewrites a file; a URI names somebody else's, and there is nothing here to edit."""
        run = run_validate(
            "refresh-links",
            "https://docs.freeports.org/en/stable/_sources/validation/general_methodology.rst.txt",
            repo=tmp_repo,
            sources=local_source,
        )
        assert run.returncode != 0, run

    def test_a_cached_copy_of_a_remote_page_is_refused(
        self, run_validate, tmp_repo, http_source, source_cache
    ):
        """The sneaky way to edit somebody else's page: reach into the cache the resolver filled.
        What comes out of it would be a page whose hash matches nothing anyone published."""
        assert (
            run_validate(
                "check-methodology",
                "basic check",
                repo=tmp_repo,
                sources=http_source.pattern,
            ).returncode
            == 0
        )
        cached = [path for path in source_cache.rglob("*") if path.is_file()]
        assert cached, "the resolver cached nothing to try this against"
        run = run_validate(
            "refresh-links",
            str(cached[0]),
            repo=tmp_repo,
            sources=http_source.pattern,
        )
        assert run.returncode != 0, run

    def test_a_file_that_is_not_there_is_refused(
        self, run_validate, tmp_repo, local_source, tmp_path
    ):
        run = run_validate(
            "refresh-links",
            str(tmp_path / "nowhere.rst"),
            repo=tmp_repo,
            sources=local_source,
        )
        assert run.returncode != 0, run

    def test_a_target_that_cannot_be_fetched_does_not_lose_its_pin(
        self, run_validate, tmp_repo, local_source, cited
    ):
        page = cited.path("methodologies/basic_check")
        page.write_text(
            page_citing(link=UNREACHABLE_URI, pins=[(UNREACHABLE_URI, "f" * 64)])
        )
        run = run_validate(
            "refresh-links",
            str(page),
            repo=tmp_repo,
            sources=local_source,
            env={"FREEPORTS_VALIDATE_OFFLINE": "1"},
        )
        assert "f" * 64 in page.read_text(), run
