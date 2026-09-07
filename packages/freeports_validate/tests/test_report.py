"""The coverage report: one model, and every rendering as a pure function of it.

`bin/collect` walks the repository's validation documents once and emits **the model** on standard
output: who has vouched for what, under which methodology, resolved from where, and whether each
claim still holds. `lib/report.py` turns that model into text, and does nothing else -- so
``report --format json | jq ...`` answers questions no rendering anticipated, and every renderer can
be tested without a repository, a keyring or a network.

The three viewpoints of the renderings are the ones the command already has as subcommands --
`who-grants` (by file), `granted-by` (by contributor), `granted-with` (by methodology) -- because a
reader who has learned one has learned the other.

Two arithmetic rules are worth stating before reading the tests that hold them:

**A methodology that declares no paths contributes no denominator.** There is nothing it could be
complete against, and inventing one would put a meaningless percentage on a badge.

**Only a document that is intact counts toward coverage.** Coverage is a statement about assurance,
and an unsigned or wrongly-signed document asserts nothing. Its grants are still *listed*, with the
state of the document they came from: nothing is hidden, and nothing uncounted is invisible.
"""

import html
import json
import xml.etree.ElementTree as ElementTree

import pytest

from conftest import BASIC_CHECK, GOLDEN_STANDARD, with_supported_paths


OUT_PATTERN = "tests/formats/*/*/out/*.csv"
OUT_PROSE = "The reference output of a format's test suite in a formats repository."
PAGES_PATTERN = "tests/formats/*/*/pages/**"
PAGES_PROSE = "The per-page fixtures of the same suite."

#: Three files the fixture repository really contains: two the pattern above covers, one it does
#: not. The variant level between the format and its outputs is part of the real layout and part of
#: the pattern, so a pattern written from memory fails here rather than passing.
FUNDS = "tests/formats/FOO-EN24/1/out/funds.csv"
INVESTMENTS = "tests/formats/FOO-EN24/1/out/investments.csv"
OUTSIDE = "content/FOO/EN24.py"


def named(model, name):
    """The one methodology entry the assertion is about."""
    return next(entry for entry in model["methodologies"] if entry["name"] == name)


# ---------------------------------------------------------------------------
# A repository with something in it to report on
# ---------------------------------------------------------------------------


@pytest.fixture
def declared_pages(methodology_pages):
    """The page tree, with `basic check` declaring paths and `golden standard` declaring none.

    Both cases have to be present in one repository, because the interesting arithmetic is what
    happens when they are added up: the second contributes grants but no denominator.
    """
    methodology_pages.write(
        "methodologies/basic_check",
        with_supported_paths(BASIC_CHECK, (OUT_PATTERN, OUT_PROSE)),
    )
    methodology_pages.write("methodologies/golden_standard", GOLDEN_STANDARD)
    return methodology_pages


@pytest.fixture
def granted_repo(tmp_repo, signer, run_validate, declared_pages):
    """A repository holding one signed document that grants two files under `basic check`.

    Built through the command rather than by writing YAML here: a fixture that assembled the
    document itself would let `create-document`, `grant` and `sign-document` break without a test
    noticing, since `collect` would still find a file shaped the way it expected.
    """
    source = declared_pages.file_pattern
    common = {"repo": tmp_repo, "key_id": signer.fingerprint, "sources": source}

    for arguments in (
        ("create-document",),
        ("sign-document",),
        # A methodology is adopted before anything is granted under it: the document records the
        # text's hash once, and the grants underneath are claims made under that text.
        ("grant", "with", "basic check"),
        ("grant", "with", "golden standard"),
        ("grant", str(tmp_repo.root / FUNDS), "with", "basic check"),
    ):
        answer = run_validate(*arguments, **common)
        assert answer.returncode == 0, answer
    return tmp_repo


@pytest.fixture
def collect(run_validate, tmp_repo, declared_pages, signer):
    """Run `collect` in the fixture repository and parse the model it printed."""

    def run(repo=None, **kwargs):
        options = {
            "repo": repo if repo is not None else tmp_repo,
            "sources": declared_pages.file_pattern,
        }
        options.update(kwargs)
        answer = run_validate("collect", **options)
        assert answer.returncode == 0, answer
        return json.loads(answer.stdout)

    return run


class TestTheModelCollectEmits:
    """What one walk of the repository's documents produces."""

    def test_it_is_json_on_standard_output(self, run_validate, granted_repo, collect):
        model = collect()
        assert isinstance(model, dict)

    def test_it_names_the_sources_it_resolved_from(
        self, granted_repo, collect, declared_pages
    ):
        assert collect()["sources"] == [declared_pages.file_pattern]

    def test_it_records_the_general_methodology_it_resolved(
        self, granted_repo, collect, declared_pages
    ):
        general = collect()["general_methodology"]
        assert general["state"] == "resolved"
        assert general["sha256"] == declared_pages.sha256("general_methodology")

    def test_a_repository_with_no_documents_is_an_empty_model_not_an_error(
        self, tmp_repo, collect
    ):
        model = collect()
        assert model["contributors"] == []
        assert model["grants"] == []
        assert model["methodologies"] == []

    def test_the_contributor_is_the_document_that_carries_the_grants(
        self, granted_repo, collect, signer
    ):
        (contributor,) = collect()["contributors"]
        assert contributor["name"] == signer.name
        assert contributor["email"] == signer.email
        assert contributor["document"] == f"validation/{signer.document_name}"

    def test_an_intact_document_is_reported_intact(self, granted_repo, collect):
        (contributor,) = collect()["contributors"]
        assert contributor["schema"] == "valid"
        assert contributor["signature"] == "valid"
        assert contributor["version"] == "current"
        assert contributor["counted"] is True

    def test_a_grant_names_its_file_its_methodology_and_its_author(
        self, granted_repo, collect, signer
    ):
        (grant,) = collect()["grants"]
        assert grant["path"] == FUNDS
        assert grant["methodology"] == "basic check"
        assert grant["contributor"] == signer.name
        assert grant["sha256"] == granted_repo.sha256(FUNDS)

    def test_an_unchanged_file_is_current(self, granted_repo, collect):
        assert collect()["grants"][0]["state"] == "current"

    def test_an_edited_file_is_changed(self, granted_repo, collect):
        granted_repo.write(FUNDS, "isin,name\nIT0001,Beta\n")
        assert collect()["grants"][0]["state"] == "changed"

    def test_a_deleted_file_is_missing(self, granted_repo, collect):
        (granted_repo.root / FUNDS).unlink()
        assert collect()["grants"][0]["state"] == "missing"

    def test_a_methodology_carries_what_it_resolved_to_and_what_it_declares(
        self, granted_repo, collect, declared_pages
    ):
        methodology = named(collect(), "basic check")
        assert methodology["state"] == "resolved"
        assert methodology["sha256"] == declared_pages.sha256(
            "methodologies/basic_check"
        )
        assert methodology["declares_paths"] is True
        assert methodology["patterns"] == [{"pattern": OUT_PATTERN, "prose": OUT_PROSE}]

    def test_an_adoption_says_whose_it_is_and_whether_it_still_holds(
        self, granted_repo, collect, signer
    ):
        (adoption,) = named(collect(), "basic check")["adoptions"]
        assert adoption["contributor"] == signer.name
        assert adoption["state"] == "current"

    def test_a_rewritten_page_makes_the_adoption_changed(
        self, granted_repo, collect, declared_pages
    ):
        declared_pages.write(
            "methodologies/basic_check",
            with_supported_paths(
                BASIC_CHECK + "\nA sentence its author added afterwards.\n",
                (OUT_PATTERN, OUT_PROSE),
            ),
        )
        assert named(collect(), "basic check")["adoptions"][0]["state"] == "changed"

    def test_a_grant_inside_the_declared_paths_is_in_scope(self, granted_repo, collect):
        assert collect()["grants"][0]["in_scope"] is True

    def test_a_grant_outside_them_is_not(
        self, granted_repo, collect, run_validate, signer, declared_pages
    ):
        forced = run_validate(
            "grant",
            "--force",
            granted_repo.root / OUTSIDE,
            "with",
            "basic check",
            repo=granted_repo,
            key_id=signer.fingerprint,
            sources=declared_pages.file_pattern,
        )
        assert forced.returncode == 0, forced
        outside = [g for g in collect()["grants"] if g["path"] == OUTSIDE]
        assert [g["in_scope"] for g in outside] == [False]

    def test_a_methodology_that_no_source_offers_is_unresolved_not_absent(
        self, granted_repo, collect, declared_pages
    ):
        declared_pages.path("methodologies/basic_check").unlink()
        model = collect()
        methodology = named(model, "basic check")
        assert methodology["state"] == "unresolved"
        assert methodology["sha256"] is None
        # Not `false`: what the page declares is unknown, not known to be nothing.
        assert methodology["declares_paths"] is None
        # The question could not be asked, which is neither yes nor no.
        assert model["grants"][0]["in_scope"] is None


class TestTheStateOfTheRunAsAWhole:
    """The single word a badge puts on a README, and how it is arrived at."""

    def test_an_intact_repository_is_passing(self, granted_repo, collect):
        assert collect()["status"] == "passing"

    def test_a_changed_file_makes_it_failing(self, granted_repo, collect):
        granted_repo.write(FUNDS, "isin,name\nIT0001,Beta\n")
        assert collect()["status"] == "failing"

    def test_a_methodology_nothing_offers_makes_it_failing(
        self, granted_repo, collect, declared_pages
    ):
        declared_pages.path("methodologies/basic_check").unlink()
        assert collect()["status"] == "failing"

    def test_a_source_that_could_not_be_reached_makes_it_inconclusive(
        self, granted_repo, run_validate
    ):
        """Offline with nothing cached: no claim was disproved, and none was confirmed either."""
        answer = run_validate(
            "collect",
            "--offline",
            repo=granted_repo,
            sources=f"http://127.0.0.1:1/{'*'}.rst",
        )
        assert answer.returncode == 0, answer
        assert json.loads(answer.stdout)["status"] == "inconclusive"


class TestCoverageArithmetic:
    """What a percentage on a badge is a percentage *of*."""

    def test_a_declaring_methodology_counts_the_files_it_could_cover(
        self, granted_repo, collect
    ):
        # Two files match `tests/formats/*/*/out/*.csv` in the fixture repository; one is granted.
        assert named(collect(), "basic check")["coverage"] == {
            "candidates": 2,
            "granted": 1,
            "ratio": 0.5,
        }

    def test_granting_the_rest_completes_it(
        self, granted_repo, collect, run_validate, signer, declared_pages
    ):
        answer = run_validate(
            "grant",
            granted_repo.root / INVESTMENTS,
            "with",
            "basic check",
            repo=granted_repo,
            key_id=signer.fingerprint,
            sources=declared_pages.file_pattern,
        )
        assert answer.returncode == 0, answer
        assert named(collect(), "basic check")["coverage"]["ratio"] == 1.0

    def test_a_methodology_declaring_no_paths_has_no_denominator(
        self, granted_repo, collect, run_validate, signer, declared_pages
    ):
        answer = run_validate(
            "grant",
            granted_repo.root / OUTSIDE,
            "with",
            "golden standard",
            repo=granted_repo,
            key_id=signer.fingerprint,
            sources=declared_pages.file_pattern,
        )
        assert answer.returncode == 0, answer
        golden = named(collect(), "golden standard")
        assert golden["declares_paths"] is False
        assert golden["coverage"] is None
        # And it does not silently join the repository's own denominator either.
        assert collect()["coverage"]["candidates"] == 2

    def test_a_file_outside_the_declared_set_is_not_a_numerator_either(
        self, granted_repo, collect, run_validate, signer, declared_pages
    ):
        answer = run_validate(
            "grant",
            "--force",
            granted_repo.root / OUTSIDE,
            "with",
            "basic check",
            repo=granted_repo,
            key_id=signer.fingerprint,
            sources=declared_pages.file_pattern,
        )
        assert answer.returncode == 0, answer
        assert named(collect(), "basic check")["coverage"]["granted"] == 1

    def test_an_unsigned_document_asserts_nothing(self, granted_repo, collect, signer):
        document = granted_repo.document(signer)
        body = document.read_text().split("sign: |-")[0].rstrip("\n")
        # `sign: ~` rather than no `sign` at all: the schema requires the field, and the state
        # being tested here is a document that is *shaped* right and vouches for nothing.
        document.write_text(f"{body}\nsign: ~\n")
        model = collect()
        assert model["contributors"][0]["counted"] is False
        assert model["coverage"]["granted"] == 0
        # Listed all the same: an uncounted grant that were also invisible would be a report
        # quietly disagreeing with the repository.
        assert len(model["grants"]) == 1


# ---------------------------------------------------------------------------
# The renderings, which know nothing but the model
# ---------------------------------------------------------------------------


def a_model(**overrides):
    """A model with everything a renderer has to cope with, as a plain dictionary.

    Written out here rather than collected from a repository because that is exactly the property
    the model exists to give: a rendering is a pure function of it, so the cases that matter to a
    renderer -- an unreachable page, a methodology with no denominator, a missing file -- are three
    lines of data instead of three repositories.
    """
    model = {
        "repository": {"root": "/home/me/formats", "name": "formats"},
        "sources": ["https://docs.example.org/validation/*.rst.txt"],
        "general_methodology": {
            "state": "resolved",
            "uri": "https://docs.example.org/validation/general_methodology.rst.txt",
            "sha256": "a" * 64,
            "origin": "network",
        },
        "contributors": [
            {
                "name": "Ada Lovelace",
                "email": "ada@example.org",
                "pubkey_id": "DEADBEEF",
                "document": "validation/ada_lovelace.yaml",
                "schema": "valid",
                "signature": "valid",
                "version": "current",
                "counted": True,
                "methodologies": ["basic check"],
                "grants": 2,
            },
            {
                "name": "Grace Hopper",
                "email": "grace@example.org",
                "pubkey_id": "CAFEBABE",
                "document": "validation/grace_hopper.yaml",
                "schema": "valid",
                "signature": "missing",
                "version": "current",
                "counted": False,
                "methodologies": ["golden standard"],
                "grants": 1,
            },
        ],
        "methodologies": [
            {
                "name": "basic check",
                "state": "resolved",
                "uri": "https://docs.example.org/validation/methodologies/basic_check.rst.txt",
                "sha256": "b" * 64,
                "origin": "network",
                "declares_paths": True,
                "patterns": [{"pattern": OUT_PATTERN, "prose": OUT_PROSE}],
                "adoptions": [
                    {
                        "contributor": "Ada Lovelace",
                        "document": "validation/ada_lovelace.yaml",
                        "sha256": "b" * 64,
                        "state": "current",
                    }
                ],
                "grants": 2,
                "coverage": {"candidates": 4, "granted": 2, "ratio": 0.5},
            },
            {
                "name": "golden standard",
                "state": "unreachable",
                "uri": "https://docs.example.org/validation/methodologies/golden_standard.rst.txt",
                "sha256": None,
                "origin": None,
                "declares_paths": False,
                "patterns": [],
                "adoptions": [
                    {
                        "contributor": "Grace Hopper",
                        "document": "validation/grace_hopper.yaml",
                        "sha256": "c" * 64,
                        "state": "unknown",
                    }
                ],
                "grants": 1,
                "coverage": None,
            },
        ],
        "grants": [
            {
                "path": FUNDS,
                "sha256": "1" * 64,
                "methodology": "basic check",
                "contributor": "Ada Lovelace",
                "document": "validation/ada_lovelace.yaml",
                "state": "current",
                "in_scope": True,
            },
            {
                "path": INVESTMENTS,
                "sha256": "2" * 64,
                "methodology": "basic check",
                "contributor": "Ada Lovelace",
                "document": "validation/ada_lovelace.yaml",
                "state": "changed",
                "in_scope": True,
            },
            {
                "path": OUTSIDE,
                "sha256": "3" * 64,
                "methodology": "golden standard",
                "contributor": "Grace Hopper",
                "document": "validation/grace_hopper.yaml",
                "state": "missing",
                "in_scope": None,
            },
        ],
        "coverage": {"candidates": 4, "granted": 2, "ratio": 0.5},
        "totals": {
            "contributors": 2,
            "methodologies": 2,
            "grants": 3,
            "files": 3,
        },
        "status": "failing",
    }
    model.update(overrides)
    return model


#: The six, without the summary: what every table has to do, whatever it groups by.
TABLE_NAMES = (
    "file-contributor",
    "file-methodology",
    "contributor-methodology",
    "contributor-file",
    "methodology-file",
    "methodology-contributor",
)


def out_of_scope_model():
    """The same model, with the third grant disproved rather than merely unasked.

    `a_model` carries `in_scope: None` there -- a page that could not be resolved -- because that is
    the case the marks exist to keep apart from a refusal. Where a test is about the refusal itself
    it needs the other value, and changing it in place would break the tests that want the first.
    """
    model = a_model()
    model["grants"] = model["grants"][:-1] + [dict(model["grants"][-1], in_scope=False)]
    return model


def rows_of(block):
    """The body rows of a Markdown table, as lists of stripped cells.

    Header and rule are dropped, and so is everything outside the table -- the summary sentence, the
    notice and the legend are prose, and a test about the table should not have to skip them by
    counting lines.
    """
    rows = []
    for line in block.splitlines():
        line = line.strip()
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        if all(set(cell) <= set("-: ") and cell for cell in cells):
            continue
        rows.append(cells)
    return rows[1:]


def row_of(block, value):
    """The one row of a Markdown table that mentions `value`, which must be exactly one."""
    matching = [row for row in rows_of(block) if any(value in cell for cell in row)]
    assert len(matching) == 1, f"{value} appears in {len(matching)} rows"
    return matching[0]


class TestTheJsonRendering:
    def test_it_is_the_model(self, report):
        model = a_model()
        assert json.loads(report.render(model, "json")) == model

    def test_it_is_readable_rather_than_compact(self, report):
        assert "\n" in report.render(a_model(), "json")


class TestTheHtmlRendering:
    @pytest.fixture
    def page(self, report):
        return report.render(a_model(), "html")

    def test_it_fetches_nothing(self, page):
        """A page that reached for a stylesheet would be a coverage report with a network outage."""
        assert "<script src=" not in page
        assert 'rel="stylesheet"' not in page
        assert "@import" not in page
        assert 'src="http' not in page

    def test_it_carries_its_own_style_and_script(self, page):
        assert "<style>" in page
        assert "<script>" in page

    def test_it_offers_the_three_viewpoints(self, page):
        assert "By file" in page
        assert "By contributor" in page
        assert "By methodology" in page

    def test_every_grant_is_on_it(self, page):
        for path in (FUNDS, INVESTMENTS, OUTSIDE):
            assert path in page

    def test_a_methodology_links_to_the_text_it_resolved_from(self, page):
        assert (
            'href="https://docs.example.org/validation/methodologies/basic_check'
            in page
        )

    def test_it_shows_the_state_of_each_grant(self, page):
        for state in ("current", "changed", "missing"):
            assert state in page

    def test_it_says_what_it_is_and_is_not(self, page, report):
        """A page of green ticks invites exactly one misreading, and has to answer it itself."""
        # Escaped, like every other piece of text on the page -- the notice goes through the same
        # escaping as a contributor's name, and a test comparing the raw string would pass only
        # for as long as nobody wrote an apostrophe into it.
        assert html.escape(report.NOTICE) in page
        assert "demonstrative" in page

    def test_it_links_to_the_general_methodology(self, page, report):
        assert f'href="{report.GENERAL_METHODOLOGY_URL}"' in page

    def test_the_notice_does_not_take_over_the_page(self, page):
        """Two sentences and a link. A caveat that dominated the report would be its own kind of
        dishonesty -- it would say the report is worthless, and it is not."""
        assert page.count("demonstrative") == 1

    def test_an_out_of_scope_grant_is_marked(self, report):
        model = a_model()
        model["grants"][0]["in_scope"] = False
        assert "(!)" in report.render(model, "html")

    def test_the_text_it_contains_is_escaped(self, report):
        model = a_model()
        model["contributors"][0]["name"] = "Ada <script>alert(1)</script>"
        page = report.render(model, "html")
        assert "<script>alert(1)</script>" not in page
        assert "&lt;script&gt;" in page

    def test_an_empty_repository_still_renders(self, report):
        empty = a_model(
            contributors=[],
            methodologies=[],
            grants=[],
            coverage={"candidates": 0, "granted": 0, "ratio": None},
            totals={"contributors": 0, "methodologies": 0, "grants": 0, "files": 0},
            status="passing",
        )
        page = report.render(empty, "html")
        assert "<body" in page.lower()
        assert "By file" in page


class TestTheBadges:
    @pytest.fixture
    def badges(self, report):
        return report.render_badges(a_model())

    def test_there_is_one_for_each_of_the_three_questions(self, badges):
        assert set(badges) == {
            "grants-total.svg",
            "grants-total.json",
            "grants-coverage.svg",
            "grants-coverage.json",
            "check-grants.svg",
            "check-grants.json",
        }

    def test_each_svg_is_well_formed_xml(self, badges):
        for name, content in badges.items():
            if name.endswith(".svg"):
                ElementTree.fromstring(content)

    def test_an_svg_needs_nothing_from_the_network(self, badges):
        for name, content in badges.items():
            if name.endswith(".svg"):
                assert "http://" not in content.replace(
                    "http://www.w3.org/2000/svg", ""
                )

    def test_the_totals_badge_says_how_many(self, badges):
        assert "3" in badges["grants-total.svg"]
        assert json.loads(badges["grants-total.json"])["message"] == "3"

    def test_the_coverage_badge_is_a_percentage(self, badges):
        assert json.loads(badges["grants-coverage.json"])["message"] == "50%"

    def test_a_repository_with_no_denominator_says_so_rather_than_zero(self, report):
        model = a_model(coverage={"candidates": 0, "granted": 0, "ratio": None})
        badges = report.render_badges(model)
        assert json.loads(badges["grants-coverage.json"])["message"] == "n/a"

    def test_the_check_badge_carries_the_status(self, badges):
        assert json.loads(badges["check-grants.json"])["message"] == "failing"

    def test_the_endpoint_json_is_what_shields_expects(self, badges):
        endpoint = json.loads(badges["check-grants.json"])
        assert endpoint["schemaVersion"] == 1
        assert set(endpoint) >= {"schemaVersion", "label", "message", "color"}

    def test_a_passing_repository_is_green_and_a_failing_one_is_not(self, report):
        passing = json.loads(
            report.render_badges(a_model(status="passing"))["check-grants.json"]
        )
        failing = json.loads(report.render_badges(a_model())["check-grants.json"])
        assert passing["color"] != failing["color"]


class TestTheMarkdownAndRstRenderings:
    """A table written between two markers, and rewritten in place."""

    def test_the_block_is_a_table_of_the_methodologies(self, report):
        block = report.render(a_model(), "markdown")
        assert "basic check" in block
        assert "golden standard" in block
        assert "|" in block

    def test_the_rst_block_is_reStructuredText_rather_than_pipes(self, report):
        block = report.render(a_model(), "rst")
        assert "basic check" in block
        assert "===" in block

    def test_both_blocks_carry_the_notice_and_its_link(self, report):
        for style in ("markdown", "rst"):
            block = report.render(a_model(), style)
            assert report.NOTICE in block
            assert report.GENERAL_METHODOLOGY_URL in block

    def test_the_notice_survives_a_rewrite(self, report):
        begin, end = report.MARKERS["markdown"]
        readme = f"# Title\n\n{begin}\n{end}\n"
        rewritten = report.rewrite(readme, report.render(a_model(), "markdown"))
        assert report.NOTICE in rewritten

    def test_the_block_carries_the_markers(self, report):
        for style in ("markdown", "rst"):
            begin, end = report.MARKERS[style]
            block = report.render(a_model(), style)
            assert block.startswith(begin)
            assert block.rstrip().endswith(end)

    def test_it_is_written_between_the_markers_and_nothing_else_moves(self, report):
        begin, end = report.MARKERS["markdown"]
        readme = f"# Title\n\nBefore.\n\n{begin}\n\nold table\n\n{end}\n\nAfter.\n"
        rewritten = report.rewrite(readme, report.render(a_model(), "markdown"))
        assert rewritten.startswith("# Title\n\nBefore.\n")
        assert rewritten.endswith("After.\n")
        assert "old table" not in rewritten
        assert "basic check" in rewritten

    def test_rewriting_twice_changes_nothing(self, report):
        begin, end = report.MARKERS["markdown"]
        readme = f"# Title\n\n{begin}\n{end}\n"
        block = report.render(a_model(), "markdown")
        once = report.rewrite(readme, block)
        assert report.rewrite(once, block) == once

    def test_an_empty_marked_region_is_filled(self, report):
        begin, end = report.MARKERS["rst"]
        page = f"Title\n=====\n\n{begin}\n{end}\n"
        assert "basic check" in report.rewrite(page, report.render(a_model(), "rst"))

    def test_missing_markers_are_an_error_rather_than_an_append(self, report):
        with pytest.raises(report.MarkersMissing):
            report.rewrite(
                "# Title\n\nNothing here.\n", report.render(a_model(), "markdown")
            )

    def test_a_half_marked_file_is_the_same_error(self, report):
        begin, _end = report.MARKERS["markdown"]
        with pytest.raises(report.MarkersMissing):
            report.rewrite(
                f"# Title\n\n{begin}\n", report.render(a_model(), "markdown")
            )


class TestTheSixLookupTables:
    """The three lookup subcommands, in both of each one's groupings, as text tables.

    Six of them, because the repository has three dimensions -- file, contributor, methodology --
    and each lookup groups by one of them and lists another. That is the same arrangement
    `who-grants`, `granted-by` and `granted-with` already have, and the names read as *grouped by
    the first, listing the second*, so nobody has to learn a second vocabulary for the same
    repository.
    """

    def test_there_are_six_of_them_besides_the_summary(self, report):
        assert set(report.TABLES) == {
            "summary",
            "file-contributor",
            "file-methodology",
            "contributor-methodology",
            "contributor-file",
            "methodology-file",
            "methodology-contributor",
        }

    def test_the_default_is_the_summary_the_renderings_always_had(self, report):
        model = a_model()
        assert report.render(model, "markdown") == report.render(
            model, "markdown", "summary"
        )

    @pytest.mark.parametrize("style", ("markdown", "rst"))
    @pytest.mark.parametrize("table", sorted(TABLE_NAMES))
    def test_every_table_renders_in_both_text_formats(self, report, style, table):
        block = report.render(a_model(), style, table)
        begin, end = report.MARKERS[style]
        assert block.startswith(begin)
        assert block.rstrip().endswith(end)

    @pytest.mark.parametrize("table", sorted(TABLE_NAMES))
    def test_every_table_says_what_it_is_worth(self, report, table):
        """A generated table that did not disclaim itself is the misreading the notice prevents."""
        for style in ("markdown", "rst"):
            block = report.render(a_model(), style, table)
            assert report.NOTICE in block
            assert report.GENERAL_METHODOLOGY_URL in block

    @pytest.mark.parametrize("table", sorted(TABLE_NAMES))
    def test_every_table_names_the_lookup_it_mirrors(self, report, table):
        """The heading is where a reader learns that this is `granted-by -f` written down."""
        block = report.render(a_model(), "markdown", table)
        assert report.TABLES[table].mirrors in block

    def test_by_file_it_lists_who_vouched_for_each_one(self, report):
        block = report.render(a_model(), "markdown", "file-contributor")
        assert row_of(block, FUNDS) == [f"`{FUNDS}`", "Ada Lovelace", "current"]

    def test_by_file_it_also_lists_under_which_methodology(self, report):
        block = report.render(a_model(), "markdown", "file-methodology")
        assert row_of(block, INVESTMENTS)[1].startswith("[basic check]")
        assert row_of(block, INVESTMENTS)[2] == "changed"

    def test_by_contributor_it_lists_the_methodologies_they_used(self, report):
        block = report.render(a_model(), "markdown", "contributor-methodology")
        assert row_of(block, "Ada Lovelace")[1].startswith("[basic check]")

    def test_a_pair_standing_for_two_grants_carries_both_their_states(self, report):
        """Ada granted two files under `basic check`, one current and one changed."""
        block = report.render(a_model(), "markdown", "contributor-methodology")
        assert row_of(block, "Ada Lovelace")[2] == "changed, current"

    def test_by_contributor_it_lists_the_files_one_row_each(self, report):
        block = report.render(a_model(), "markdown", "contributor-file")
        assert row_of(block, FUNDS) == ["Ada Lovelace", f"`{FUNDS}`", "current"]
        assert row_of(block, INVESTMENTS) == [
            "Ada Lovelace",
            f"`{INVESTMENTS}`",
            "changed",
        ]

    def test_by_methodology_it_lists_the_files_it_covers(self, report):
        block = report.render(a_model(), "markdown", "methodology-file")
        assert row_of(block, FUNDS)[0].startswith("[basic check]")
        assert row_of(block, FUNDS)[1] == f"`{FUNDS}`"

    def test_by_methodology_it_lists_who_adopted_it(self, report):
        block = report.render(a_model(), "markdown", "methodology-contributor")
        assert row_of(block, "Grace Hopper")[1].startswith("Grace Hopper")

    def test_the_group_key_is_repeated_rather_than_blanked(self, report):
        """Ada has two files, so her name is on both rows: a blank cell is not greppable."""
        block = report.render(a_model(), "markdown", "contributor-file")
        assert sum(1 for row in rows_of(block) if row[0] == "Ada Lovelace") == 2

    def test_the_rows_are_sorted_by_the_group_then_the_listed_item(self, report):
        block = report.render(a_model(), "markdown", "methodology-file")
        assert rows_of(block) == sorted(rows_of(block))

    def test_a_pair_appearing_twice_is_one_row(self, report):
        """The same file granted under the same methodology by two documents is one claim here."""
        model = a_model()
        twice = dict(model["grants"][0], document="validation/second.yaml")
        model["grants"] = model["grants"] + [twice]
        block = report.render(model, "markdown", "file-contributor")
        assert len([row for row in rows_of(block) if row[0] == f"`{FUNDS}`"]) == 1


class TestTheScopeMarksOnTheTables:
    """`(!)` and `(?)`, on the listed item and never on the group key."""

    def test_a_page_that_could_not_be_asked_marks_the_listed_item(self, report):
        block = report.render(a_model(), "markdown", "methodology-file")
        assert row_of(block, OUTSIDE)[1].endswith("(?)")

    def test_and_not_the_group_key(self, report):
        """Marking the heading would split one methodology in two the moment a grant left scope."""
        block = report.render(a_model(), "markdown", "methodology-file")
        assert "(?)" not in row_of(block, OUTSIDE)[0]

    def test_an_out_of_scope_grant_is_marked_with_the_exclamation(self, report):
        block = report.render(out_of_scope_model(), "markdown", "methodology-file")
        assert row_of(block, OUTSIDE)[1].endswith("(!)")

    def test_the_legend_is_printed_when_a_mark_appears(self, report):
        block = report.render(out_of_scope_model(), "markdown", "methodology-file")
        assert report.OUT_OF_SCOPE_LEGEND in block

    def test_and_not_when_none_does(self, report):
        """A legend for a mark nobody can see is noise in a file kept under version control."""
        model = a_model()
        model["grants"] = [dict(grant, in_scope=True) for grant in model["grants"]]
        block = report.render(model, "markdown", "methodology-file")
        assert report.OUT_OF_SCOPE_LEGEND not in block
        assert report.UNRESOLVED_LEGEND not in block

    def test_the_two_marks_have_a_line_each(self, report):
        model = out_of_scope_model()
        model["grants"] = model["grants"] + [
            dict(model["grants"][0], path="content/other.py", in_scope=None)
        ]
        block = report.render(model, "markdown", "methodology-file")
        assert report.OUT_OF_SCOPE_LEGEND in block
        assert report.UNRESOLVED_LEGEND in block

    def test_out_of_scope_wins_over_unasked_within_one_pair(self, report):
        """A pair with a disproved grant and an unasked one has been disproved."""
        model = out_of_scope_model()
        model["grants"] = model["grants"] + [
            dict(model["grants"][-1], document="validation/second.yaml", in_scope=None)
        ]
        block = report.render(model, "markdown", "methodology-file")
        assert row_of(block, OUTSIDE)[1].endswith("(!)")


class TestTheTablesOnAnEmptyRepository:
    """Nothing to report is a sentence, not a traceback."""

    @pytest.mark.parametrize("table", sorted(TABLE_NAMES))
    @pytest.mark.parametrize("style", ("markdown", "rst"))
    def test_a_repository_with_no_grants_still_renders(self, report, table, style):
        model = a_model(
            contributors=[],
            methodologies=[],
            grants=[],
            totals={"contributors": 0, "methodologies": 0, "grants": 0, "files": 0},
        )
        block = report.render(model, style, table)
        assert report.NOTICE in block
        assert report.MARKERS[style][1] in block

    @pytest.mark.parametrize("table", sorted(TABLE_NAMES))
    def test_an_empty_table_is_rewritable_like_any_other(self, report, table):
        begin, end = report.MARKERS["markdown"]
        model = a_model(grants=[])
        page = f"# Title\n\n{begin}\n{end}\n"
        assert report.rewrite(page, report.render(model, "markdown", table))


class TestARefusedTable:
    def test_a_table_the_json_rendering_cannot_have_is_refused(self, report):
        """`json` is the model; a table of it would be a second, lossy model."""
        with pytest.raises(KeyError):
            report.render(a_model(), "json", "methodology-file")

    def test_an_unknown_table_name_is_refused(self, report):
        with pytest.raises(KeyError):
            report.render(a_model(), "markdown", "by-phase-of-the-moon")


class TestTheRendererAsAProcess:
    """The command-line contract `bin/report` depends on."""

    def test_it_reads_the_model_on_stdin_and_writes_on_stdout(self, report_script):
        answer = report_script("json", stdin=json.dumps(a_model()))
        assert answer.returncode == 0, answer
        assert json.loads(answer.stdout)["status"] == "failing"

    def test_it_writes_to_a_file_when_told_to(self, report_script, tmp_path):
        out = tmp_path / "coverage.html"
        answer = report_script("html", "--out", out, stdin=json.dumps(a_model()))
        assert answer.returncode == 0, answer
        assert "<style>" in out.read_text()

    def test_badges_go_into_a_directory(self, report_script, tmp_path):
        out = tmp_path / "badges"
        answer = report_script("badges", "--out", out, stdin=json.dumps(a_model()))
        assert answer.returncode == 0, answer
        assert (out / "check-grants.svg").exists()
        assert (out / "grants-coverage.json").exists()

    def test_badges_without_a_directory_are_refused_rather_than_guessed(
        self, report_script
    ):
        answer = report_script("badges", stdin=json.dumps(a_model()))
        assert answer.returncode != 0
        assert "--out" in answer.output

    def test_a_table_is_chosen_by_name(self, report_script):
        answer = report_script(
            "markdown", "--table", "methodology-file", stdin=json.dumps(a_model())
        )
        assert answer.returncode == 0, answer
        assert "granted-with <methodology>" in answer.stdout

    def test_an_unknown_table_names_the_ones_there_are(self, report_script):
        answer = report_script(
            "markdown", "--table", "by-phase-of-the-moon", stdin=json.dumps(a_model())
        )
        assert answer.returncode != 0
        assert "methodology-file" in answer.output

    def test_a_table_of_a_format_that_has_none_is_refused(self, report_script):
        """Silently rendering the summary instead is how a hook writes the wrong file for a year."""
        answer = report_script(
            "json", "--table", "methodology-file", stdin=json.dumps(a_model())
        )
        assert answer.returncode != 0
        assert "markdown" in answer.output

    def test_an_unknown_format_names_the_ones_there_are(self, report_script):
        answer = report_script("cuneiform", stdin=json.dumps(a_model()))
        assert answer.returncode != 0
        assert "markdown" in answer.output

    def test_a_missing_marker_is_reported_where_a_person_can_act_on_it(
        self, report_script, tmp_path
    ):
        readme = tmp_path / "README.md"
        readme.write_text("# Title\n")
        answer = report_script("markdown", "--out", readme, stdin=json.dumps(a_model()))
        assert answer.returncode != 0
        assert "freeports-validate:begin" in answer.output
        assert readme.read_text() == "# Title\n"


# ---------------------------------------------------------------------------
# The subcommand
# ---------------------------------------------------------------------------


class TestTheReportSubcommand:
    def test_it_defaults_to_the_model(self, granted_repo, run_validate, declared_pages):
        answer = run_validate(
            "report", repo=granted_repo, sources=declared_pages.file_pattern
        )
        assert answer.returncode == 0, answer
        assert json.loads(answer.stdout)["totals"]["grants"] == 1

    def test_it_writes_a_page(
        self, granted_repo, run_validate, declared_pages, tmp_path
    ):
        out = tmp_path / "coverage.html"
        answer = run_validate(
            "report",
            "--format",
            "html",
            "--out",
            out,
            repo=granted_repo,
            sources=declared_pages.file_pattern,
        )
        assert answer.returncode == 0, answer
        assert FUNDS in out.read_text()

    def test_it_writes_badges(
        self, granted_repo, run_validate, declared_pages, tmp_path
    ):
        out = tmp_path / "badges"
        answer = run_validate(
            "report",
            "--format",
            "badges",
            "--out",
            out,
            repo=granted_repo,
            sources=declared_pages.file_pattern,
        )
        assert answer.returncode == 0, answer
        assert (out / "check-grants.svg").exists()

    def test_it_rewrites_a_readme_between_its_markers(
        self, granted_repo, run_validate, declared_pages
    ):
        readme = granted_repo.root / "README.md"
        readme.write_text(
            "# A formats repository\n\n"
            "<!-- freeports-validate:begin -->\n<!-- freeports-validate:end -->\n"
        )
        answer = run_validate(
            "report",
            "--format",
            "markdown",
            "--out",
            readme,
            repo=granted_repo,
            sources=declared_pages.file_pattern,
        )
        assert answer.returncode == 0, answer
        assert "basic check" in readme.read_text()

    def test_it_writes_one_of_the_six_tables(
        self, granted_repo, run_validate, declared_pages
    ):
        page = granted_repo.root / "by-methodology.md"
        page.write_text(
            "# Covered\n\n"
            "<!-- freeports-validate:begin -->\n<!-- freeports-validate:end -->\n"
        )
        answer = run_validate(
            "report",
            "--format",
            "markdown",
            "--table",
            "methodology-file",
            "--out",
            page,
            repo=granted_repo,
            sources=declared_pages.file_pattern,
        )
        assert answer.returncode == 0, answer
        assert f"`{FUNDS}`" in page.read_text()

    def test_an_unknown_table_is_refused(
        self, granted_repo, run_validate, declared_pages
    ):
        answer = run_validate(
            "report",
            "--table",
            "by-phase-of-the-moon",
            repo=granted_repo,
            sources=declared_pages.file_pattern,
        )
        assert answer.returncode != 0

    def test_a_model_already_collected_is_rendered_without_walking_again(
        self, granted_repo, run_validate, declared_pages, tmp_path
    ):
        """Eight artefacts out of one repository must walk it once, not eight times."""
        model = tmp_path / "model.json"
        collected = run_validate(
            "collect", repo=granted_repo, sources=declared_pages.file_pattern
        )
        assert collected.returncode == 0, collected
        model.write_text(collected.stdout)

        answer = run_validate(
            "report",
            "--model",
            model,
            "--format",
            "html",
            repo=granted_repo,
            sources="/nowhere/that/exists/*.rst",
        )
        assert answer.returncode == 0, answer
        assert FUNDS in answer.stdout

    def test_a_model_that_is_not_there_is_refused_rather_than_collected(
        self, granted_repo, run_validate, declared_pages, tmp_path
    ):
        """Falling back to a walk would answer a question about a file nobody wrote."""
        answer = run_validate(
            "report",
            "--model",
            tmp_path / "absent.json",
            repo=granted_repo,
            sources=declared_pages.file_pattern,
        )
        assert answer.returncode != 0
        assert "absent.json" in answer.output

    def test_the_model_may_come_down_a_pipe(
        self, granted_repo, run_validate, declared_pages
    ):
        collected = run_validate(
            "collect", repo=granted_repo, sources=declared_pages.file_pattern
        )
        answer = run_validate(
            "report",
            "--model",
            "-",
            "--format",
            "markdown",
            stdin=collected.stdout,
            repo=granted_repo,
            sources="/nowhere/that/exists/*.rst",
        )
        assert answer.returncode == 0, answer
        assert "basic check" in answer.stdout

    def test_it_needs_no_signing_key(self, granted_repo, run_validate, declared_pages):
        """Auditing a repository you did not write is the case this exists for."""
        answer = run_validate(
            "report", repo=granted_repo, sources=declared_pages.file_pattern
        )
        assert answer.returncode == 0, answer

    def test_an_unknown_format_is_refused(
        self, granted_repo, run_validate, declared_pages
    ):
        answer = run_validate(
            "report",
            "--format",
            "cuneiform",
            repo=granted_repo,
            sources=declared_pages.file_pattern,
        )
        assert answer.returncode != 0


# ---------------------------------------------------------------------------
# The size a real repository reaches
# ---------------------------------------------------------------------------

#: Linux caps a *single* argument at 32 pages, whatever room the whole command line has. It is not
#: `ARG_MAX`, which is megabytes and about the command line as a whole, and that difference is the
#: whole of the bug this section exists for: the model fitted comfortably in `ARG_MAX` and still
#: could not be passed as one argument.
MAX_ARG_STRLEN = 128 * 1024

#: Enough granted files for the model's `grants` array to pass that limit when serialised. A
#: formats repository reached it by vouching for one test suite -- 571 files -- so this is not a
#: hypothetical size, and a test that stayed under it would guard nothing.
AT_SCALE = 700


@pytest.fixture
def repo_granted_at_scale(tmp_repo, signer, run_validate, declared_pages):
    """A repository whose whole test suite has been vouched for, in one grant.

    Written as a single `grant` invocation because that is how it happens: a granter names a
    directory's worth of output and signs once. It is also the shape that made both defects
    visible -- the write that was quadratic in the number of files, and the model that could not be
    assembled afterwards.
    """
    paths = []
    for index in range(AT_SCALE):
        relative = f"tests/formats/FOO-EN24/1/out/investments_{index:04d}.csv"
        tmp_repo.write(relative, f"isin,value\nIT{index:08d},{index}\n")
        paths.append(str(tmp_repo.root / relative))

    common = {
        "repo": tmp_repo,
        "key_id": signer.fingerprint,
        "sources": declared_pages.file_pattern,
    }
    for arguments in (
        ("create-document",),
        ("sign-document",),
        ("grant", "with", "basic check"),
    ):
        answer = run_validate(*arguments, **common)
        assert answer.returncode == 0, answer

    granted = run_validate("grant", *paths, "with", "basic check", **common)
    assert granted.returncode == 0, granted
    return tmp_repo


class TestARepositoryVouchedForAtScale:
    """What `collect` does once the model is bigger than one argument may be.

    Every assertion here holds trivially for the three-file repository the rest of this file uses.
    They are worth writing only at a size the command actually meets, which is why the fixture
    grants a whole suite rather than adding one file to the small one.
    """

    def test_the_grants_are_past_the_single_argument_limit(
        self, repo_granted_at_scale, collect
    ):
        """The premise of the two tests below, asserted rather than assumed.

        If a later change makes the fixture smaller, this fails and says so, instead of leaving two
        tests that pass without exercising anything.
        """
        model = collect(repo=repo_granted_at_scale)
        assert len(json.dumps(model["grants"])) > MAX_ARG_STRLEN

    def test_the_model_is_still_emitted(self, repo_granted_at_scale, collect):
        model = collect(repo=repo_granted_at_scale)
        assert model["totals"]["grants"] == AT_SCALE
        assert model["totals"]["files"] == AT_SCALE

    def test_every_granted_file_is_current(self, repo_granted_at_scale, collect):
        """The grants are read back as claims about the files, not merely counted."""
        states = {
            grant["state"] for grant in collect(repo=repo_granted_at_scale)["grants"]
        }
        assert states == {"current"}

    def test_coverage_counts_them(self, repo_granted_at_scale, collect):
        coverage = named(collect(repo=repo_granted_at_scale), "basic check")["coverage"]
        assert coverage["granted"] == AT_SCALE
        assert coverage["candidates"] >= AT_SCALE
