"""Making a manifest's hash of its own contents move together with its declared version.

Three repositories in this workspace declare a fingerprint and, until this work, **nothing computed
it**. A field nobody verifies is not a fingerprint; it is a comment that looks like one.

The recipe has to be reproducible by hand, or the first person it refuses will not believe it:

    printf '%s\\n' <paths> | LC_ALL=C sort | xargs sha256sum | sha256sum

Two properties of it are deliberate and are pinned here. **The path is inside the hashed lines**, so
moving a file changes the fingerprint as much as editing it — a grant naming `tests/a.json` does not
cover the same repository once that file is called `tests/b.json`. And the sort is byte order, not
the committer's locale, because the one thing a fingerprint must never do is differ between two
people looking at the same tree.

The rule the fingerprint exists for is the version. When the covered content moves, the declared
version must move with it; otherwise the manifest says version X covers content Y, which is false,
and the point of the field is that it is not. On prod that refuses the commit; on dev it warns and
**does not write the new fingerprint**, because writing it is precisely how the manifest would come
to hold that false statement.

`companies/` decides what *matches* and `lists/` only decides the *association*, which is why a
companies change is a major bump and subsumes a lists one.
"""

import subprocess

import pytest

from freeports_dev.ci import fingerprint as fp
from freeports_dev.ci.manifest import Manifest, ManifestError


def git(root, *arguments):
    subprocess.run(
        ["git", "-C", str(root), *arguments], check=True, capture_output=True
    )


def commit(root, message="c"):
    git(root, "add", "-A")
    git(root, "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", message)


@pytest.fixture
def database(tmp_path):
    """An input database with both directories, committed, and consistent with itself."""
    for name in ("companies", "lists"):
        (tmp_path / name).mkdir()
        (tmp_path / name / f"{name}.csv").write_text(f"{name}\nrow\n")
    (tmp_path / "metadata.yaml").write_text(
        "schema: v0.0.0\n"
        "info:\n"
        '  description: "A database"\n'
        "  version: v1.2.3\n"
        "sha256:\n"
        f"  companies: {fp.fingerprint(tmp_path, fp.files_under(tmp_path, 'companies'))}\n"
        f"  lists: {fp.fingerprint(tmp_path, fp.files_under(tmp_path, 'lists'))}\n"
    )
    git(tmp_path, "init", "-q", "-b", "main")
    commit(tmp_path, "seed")
    return tmp_path


class TestTheRecipe:
    def test_it_is_the_hash_of_the_hashes_and_not_of_the_contents(self, tmp_path):
        (tmp_path / "a").write_text("one")
        direct = fp.hash_file(tmp_path / "a")
        assert fp.fingerprint(tmp_path, ["a"]) != direct

    def test_editing_a_file_moves_it(self, tmp_path):
        (tmp_path / "a").write_text("one")
        before = fp.fingerprint(tmp_path, ["a"])
        (tmp_path / "a").write_text("two")
        assert fp.fingerprint(tmp_path, ["a"]) != before

    def test_moving_a_file_moves_it_just_as_much(self, tmp_path):
        """The path is inside the hashed lines on purpose."""
        (tmp_path / "a").write_text("one")
        as_a = fp.fingerprint(tmp_path, ["a"])
        (tmp_path / "b").write_text("one")
        assert fp.fingerprint(tmp_path, ["b"]) != as_a

    def test_the_order_the_paths_arrive_in_does_not_matter(self, tmp_path):
        (tmp_path / "a").write_text("one")
        (tmp_path / "b").write_text("two")
        assert fp.fingerprint(tmp_path, ["a", "b"]) == fp.fingerprint(
            tmp_path, ["b", "a"]
        )

    def test_adding_a_file_moves_it(self, tmp_path):
        (tmp_path / "a").write_text("one")
        before = fp.fingerprint(tmp_path, ["a"])
        (tmp_path / "b").write_text("two")
        assert fp.fingerprint(tmp_path, ["a", "b"]) != before

    def test_an_empty_file_set_still_has_an_answer(self, tmp_path):
        assert len(fp.fingerprint(tmp_path, [])) == 64

    def test_it_matches_the_shell_one_liner_the_documentation_prints(self, tmp_path):
        """If these two ever disagree, the documented recipe is a lie and nobody can check a refusal."""
        (tmp_path / "a").write_text("one")
        (tmp_path / "b").write_text("two")
        shell = subprocess.run(
            "printf '%s\\n' a b | LC_ALL=C sort | xargs sha256sum | sha256sum",
            shell=True,
            cwd=str(tmp_path),
            capture_output=True,
            text=True,
            check=True,
        )
        assert fp.fingerprint(tmp_path, ["a", "b"]) == shell.stdout.split()[0]

    def test_a_directory_contributes_nothing_of_its_own(self, tmp_path):
        (tmp_path / "d").mkdir()
        (tmp_path / "d" / "a").write_text("one")
        assert fp.files_under(tmp_path, "d") == ["d/a"]

    def test_an_absent_directory_holds_no_files_rather_than_failing(self, tmp_path):
        assert fp.files_under(tmp_path, "nothing") == []


class TestReadingWhatAGrantCovers:
    def test_the_files_come_from_the_repository_s_own_validation_documents(
        self, tmp_path
    ):
        """Read from disk, not through `collect`: a fingerprint must be computable on a train."""
        (tmp_path / "validation").mkdir()
        (tmp_path / "a.json").write_text("{}")
        (tmp_path / "validation" / "who.yaml").write_text(
            "data:\n  - methodology: basic\n    files:\n      - path: a.json\n        sha256: x\n"
        )
        assert fp.granted_files(tmp_path) == ["a.json"]

    def test_a_file_a_document_names_but_that_is_gone_is_not_hashed(self, tmp_path):
        (tmp_path / "validation").mkdir()
        (tmp_path / "validation" / "who.yaml").write_text(
            "data:\n  - methodology: basic\n    files:\n      - path: gone.json\n        sha256: x\n"
        )
        assert fp.granted_files(tmp_path) == []

    def test_a_file_named_by_two_documents_is_hashed_once(self, tmp_path):
        (tmp_path / "validation").mkdir()
        (tmp_path / "a.json").write_text("{}")
        for name in ("one", "two"):
            (tmp_path / "validation" / f"{name}.yaml").write_text(
                "data:\n  - methodology: basic\n    files:\n      - path: a.json\n        sha256: x\n"
            )
        assert fp.granted_files(tmp_path) == ["a.json"]

    def test_a_repository_with_no_validation_directory_covers_nothing(self, tmp_path):
        assert fp.granted_files(tmp_path) == []


class TestTheBumpRules:
    def test_a_change_under_lists_is_a_minor_bump(self):
        """A list change alters only which companies a list associates."""
        assert fp.bump_kind({"lists"}) == fp.MINOR
        assert fp.bump((1, 2, 3), fp.MINOR) == (1, 3, 0)

    def test_a_change_under_companies_is_a_major_bump(self):
        """`companies/` decides what matches, so it changes the meaning of every run."""
        assert fp.bump_kind({"companies"}) == fp.MAJOR
        assert fp.bump((1, 2, 3), fp.MAJOR) == (2, 0, 0)

    def test_both_together_is_still_a_major_bump(self):
        """A companies change subsumes a lists one rather than being a second change beside it."""
        assert fp.bump_kind({"companies", "lists"}) == fp.MAJOR

    def test_a_minor_bump_resets_the_patch(self):
        assert fp.bump((1, 2, 3), fp.MINOR) == (1, 3, 0)

    def test_a_major_bump_resets_both(self):
        assert fp.bump((1, 2, 3), fp.MAJOR) == (2, 0, 0)

    def test_nothing_moved_proposes_nothing(self):
        assert fp.bump_kind(set()) is None

    def test_the_v_prefix_is_kept_if_it_was_there_and_not_invented_if_it_was_not(self):
        assert fp.format_version((1, 0, 0), "v0.0.0") == "v1.0.0"
        assert fp.format_version((1, 0, 0), "0.0.0") == "1.0.0"

    def test_a_version_this_tool_should_not_rewrite_is_not_parsed(self):
        """`2024.3` is somebody's scheme, not a semver this tool gets to guess at."""
        assert fp.parse_version("2024.3") is None
        assert fp.parse_version("v1.2.3") == (1, 2, 3)


class TestTheRuleAgainstTheCommitAtHead:
    def test_a_repository_consistent_with_itself_is_ok(self, database):
        _, check = fp.check_input_db(database)
        assert check.ok
        assert not check.moved

    def test_a_changed_file_moves_the_fingerprint(self, database):
        (database / "lists" / "lists.csv").write_text("lists\nrow\nanother\n")
        _, check = fp.check_input_db(database)
        assert check.changed_directories == {"lists"}
        assert not check.ok

    def test_and_it_is_compared_against_head_not_against_the_working_tree(
        self, database
    ):
        """The hook rewrites the manifest itself, so the working tree would always agree."""
        (database / "lists" / "lists.csv").write_text("lists\nrow\nanother\n")
        manifest, _ = fp.check_input_db(database)
        manifest.set(
            ["sha256", "lists"],
            fp.fingerprint(database, fp.files_under(database, "lists")),
        )
        manifest.write()
        _, check = fp.check_input_db(database)
        assert check.changed_directories == {"lists"}

    def test_moving_the_version_too_makes_the_claim_true_again(self, database):
        (database / "lists" / "lists.csv").write_text("lists\nrow\nanother\n")
        manifest, _ = fp.check_input_db(database)
        manifest.set(["info", "version"], "v1.3.0")
        manifest.write()
        _, check = fp.check_input_db(database)
        assert check.ok

    def test_the_proposal_follows_the_directory_that_moved(self, database):
        (database / "lists" / "lists.csv").write_text("lists\nrow\nanother\n")
        _, check = fp.check_input_db(database)
        assert check.proposal() == (fp.MINOR, "v1.3.0")

    def test_and_a_companies_change_proposes_a_major_one(self, database):
        (database / "companies" / "companies.csv").write_text(
            "companies\nrow\nanother\n"
        )
        _, check = fp.check_input_db(database)
        assert check.proposal() == (fp.MAJOR, "v2.0.0")

    def test_there_is_nothing_to_propose_once_the_version_has_moved(self, database):
        (database / "lists" / "lists.csv").write_text("lists\nrow\nanother\n")
        manifest, _ = fp.check_input_db(database)
        manifest.set(["info", "version"], "v9.9.9")
        manifest.write()
        _, check = fp.check_input_db(database)
        assert check.proposal() is None

    def test_a_repository_with_no_commit_has_no_earlier_claim_to_contradict(
        self, tmp_path
    ):
        for name in ("companies", "lists"):
            (tmp_path / name).mkdir()
            (tmp_path / name / "a.csv").write_text("a\n")
        (tmp_path / "metadata.yaml").write_text(
            "schema: v0.0.0\ninfo:\n  version: v0.0.0\nsha256:\n  companies: x\n  lists: y\n"
        )
        git(tmp_path, "init", "-q", "-b", "main")
        _, check = fp.check_input_db(tmp_path)
        assert check.version_moved
        assert check.ok


class TestAFormatsRepository:
    def test_its_fingerprint_covers_every_granted_file(self, tmp_path):
        (tmp_path / "metadata").mkdir()
        (tmp_path / "metadata" / "formats.csv").write_text("Name,Locale,Year\n")
        (tmp_path / "validation").mkdir()
        (tmp_path / "a.json").write_text("{}")
        (tmp_path / "validation" / "who.yaml").write_text(
            "data:\n  - methodology: basic\n    files:\n      - path: a.json\n        sha256: x\n"
        )
        (tmp_path / "package.yaml").write_text(
            'schema: v0.0.0\ninfo:\n  description: "d"\n  version: v0.0.0\n'
            "  validation_sha256: nothing\n"
        )
        git(tmp_path, "init", "-q", "-b", "main")
        commit(tmp_path)
        _, check = fp.check_formats(tmp_path)
        assert check.entries[0]["computed"] == fp.fingerprint(tmp_path, ["a.json"])
        assert not check.ok

    def test_which_version_to_move_is_the_author_s_choice_so_nothing_is_proposed(
        self, tmp_path
    ):
        """The change may be a correction or a whole new format, and only they know which."""
        (tmp_path / "metadata").mkdir()
        (tmp_path / "metadata" / "formats.csv").write_text("Name,Locale,Year\n")
        (tmp_path / "package.yaml").write_text(
            'schema: v0.0.0\ninfo:\n  description: "d"\n  version: v0.0.0\n'
            "  validation_sha256: nothing\n"
        )
        git(tmp_path, "init", "-q", "-b", "main")
        commit(tmp_path)
        _, check = fp.check_formats(tmp_path)
        assert check.proposal() is None


class TestRewritingAManifest:
    def test_a_field_is_replaced_and_nothing_else_is_touched(self, tmp_path):
        path = tmp_path / "package.yaml"
        path.write_text(
            "# a comment nobody asked to lose\n"
            "schema: v0.0.0\n"
            "info:\n"
            '  description: "Italy - 5 top groups"\n'
            "  version: v0.0.0\n"
        )
        manifest = Manifest(path)
        manifest.set(["info", "version"], "v1.0.0")
        manifest.write()
        text = path.read_text()
        assert "# a comment nobody asked to lose" in text
        assert '"Italy - 5 top groups"' in text
        assert "version: v1.0.0" in text

    def test_a_nested_field_is_not_confused_with_a_top_level_one_of_the_same_name(
        self, tmp_path
    ):
        path = tmp_path / "m.yaml"
        path.write_text("version: top\ninfo:\n  version: nested\n")
        manifest = Manifest(path)
        manifest.set(["info", "version"], "changed")
        manifest.write()
        assert path.read_text() == "version: top\ninfo:\n  version: changed\n"

    def test_the_change_is_visible_immediately_through_get(self, tmp_path):
        path = tmp_path / "m.yaml"
        path.write_text("info:\n  version: v0.0.0\n")
        manifest = Manifest(path)
        manifest.set(["info", "version"], "v1.0.0")
        assert manifest.get("info", "version") == "v1.0.0"

    def test_a_field_that_is_not_there_is_named_rather_than_invented(self, tmp_path):
        path = tmp_path / "m.yaml"
        path.write_text("info:\n  version: v0.0.0\n")
        with pytest.raises(ManifestError) as raised:
            Manifest(path).set(["info", "validation_sha256"], "x")
        assert "validation_sha256" in str(raised.value)

    def test_a_manifest_that_is_not_there(self, tmp_path):
        with pytest.raises(ManifestError):
            Manifest(tmp_path / "gone.yaml")

    def test_a_manifest_that_does_not_parse(self, tmp_path):
        path = tmp_path / "m.yaml"
        path.write_text("info: [oh: no\n")
        with pytest.raises(ManifestError):
            Manifest(path)
