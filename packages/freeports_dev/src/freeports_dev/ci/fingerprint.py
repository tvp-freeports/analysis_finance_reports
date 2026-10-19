"""``freeports-dev fingerprint``: making a manifest's hash and its version move together.

Three repositories in this workspace declare a fingerprint of their own contents and, until this
work, **nothing computed it**. ``info.validation_sha256`` in a formats repository's
``package.yaml``, and ``sha256.companies`` / ``sha256.lists`` in an input database's
``metadata.yaml``, were written down once and never checked. A field nobody verifies is not a
fingerprint, it is a comment that looks like one.

The recipe is one function, used by both repository kinds, and it is short enough to print in the
documentation so that anybody can reproduce it by hand::

    printf '%s\\n' <paths> | LC_ALL=C sort | xargs sha256sum | sha256sum

That is: the paths, relative to the repository root, sorted in ``LC_ALL=C`` order; the sha256 of
each, in that order, as ``"<hash>  <path>"`` lines; and the sha256 of that stream.

**The path is inside the hashed lines on purpose.** Moving a file changes the fingerprint as much
as editing it, which is what was asked for and is right: a grant that names ``tests/a.json`` does
not cover the same repository once that file is called ``tests/b.json``. Sorting is ``LC_ALL=C`` so
that the answer does not depend on the committer's locale — the one thing a fingerprint must never
do is differ between two people looking at the same tree.

Non-ASCII file names would need ``sha256sum``'s own escaping rules pinned down to stay
reproducible. Both commit hooks already refuse them, so this says so rather than pretending to
handle a case nobody can currently create.

**What the fingerprint is for is the version.** When the covered content moves, the declared
version must move with it; otherwise the manifest claims that version X covers content Y, which is
a false statement, and the whole point of the field is that it is not one. On a prod branch that
refuses the commit. On dev it warns loudly and — deliberately — **does not write the new
fingerprint**, because writing it is exactly how the manifest would come to hold that false
statement.
"""

import hashlib
import re
from pathlib import Path


#: How the two-space-separated lines `sha256sum` prints are reproduced, byte for byte.
_LINE = "{hash}  {path}\n"


def hash_file(path):
    """The sha256 of one file, read in chunks so a large PDF does not have to fit in memory."""
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def listing(root, relative_paths):
    """The recipe's intermediate stream: one ``"<hash>  <path>"`` line per file, sorted.

    It is exactly what ``xargs sha256sum`` prints, so a saved listing is checked by hand with
    ``sha256sum -c`` and its own ``sha256sum`` is the fingerprint.

    Sorted with :func:`sorted` over the raw strings, which is Python's byte-order comparison and
    therefore the same order ``LC_ALL=C sort`` produces. Naming the locale in the shell one-liner
    and relying on the default here would be two recipes that agree until somebody's machine
    disagrees.
    """
    root = Path(root)
    return "".join(
        _LINE.format(hash=hash_file(root / relative), path=relative)
        for relative in sorted(str(p) for p in relative_paths)
    )


def fingerprint(root, relative_paths):
    """The hash of the hashes, over a set of paths relative to ``root``."""
    return hash_text(listing(root, relative_paths))


def hash_text(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def _is_cache(part):
    """A directory a tool left behind: ``__pycache__``, or a hidden one (``.ruff_cache``).

    Their contents differ from machine to machine, so a fingerprint that counted them would differ
    between two people looking at the same tree. Only *directories* are skipped: a hidden file such
    as a format test's ``.log.csv`` is an expected output, not a cache.
    """
    return part == "__pycache__" or part.startswith(".")


def files_under(root, directory):
    """Every file under one directory of the repository, as paths relative to the root.

    Directories are not hashed and empty ones therefore do not exist as far as a fingerprint is
    concerned, which matches what git records and means the fingerprint does not move for a
    difference git will not carry anyway. Cache directories (:func:`_is_cache`) are skipped.
    """
    root = Path(root)
    base = root / directory
    if not base.is_dir():
        return []
    return sorted(
        str(path.relative_to(root))
        for path in base.rglob("*")
        if path.is_file()
        and not any(_is_cache(part) for part in path.relative_to(base).parts[:-1])
    )


def _parse_listing(text):
    entries = {}
    for line in (text or "").splitlines():
        digest, _, path = line.partition("  ")
        if path:
            entries[path] = digest
    return entries


def listing_changes(old_text, new_text):
    """Which paths two listings add, change and remove, each sorted."""
    old, new = _parse_listing(old_text), _parse_listing(new_text)
    return {
        "added": sorted(set(new) - set(old)),
        "changed": sorted(p for p in set(old) & set(new) if old[p] != new[p]),
        "removed": sorted(set(old) - set(new)),
    }


def listing_kind(old_text, new_text):
    """The bump a change of expected test outputs calls for.

    Only new files: the new fixtures cover behaviour that already existed, and no earlier output
    changes -- a minor bump. Any file changed or removed: what a run produced before is no longer
    what it produces -- major. With no earlier listing there is nothing to compare, and this tool
    does not guess.
    """
    if old_text is None:
        return None
    changes = listing_changes(old_text, new_text)
    if changes["changed"] or changes["removed"]:
        return MAJOR
    if changes["added"]:
        return MINOR
    return None


def granted_files(root):
    """Every file covered by a grant, read from the repository's own ``validation/*.yaml``.

    Read from the documents and **not** from ``freeports-validate collect``: collecting resolves
    the methodology pages over the network, and a fingerprint that could not be computed on a train
    is a fingerprint the commit hook has to skip. The documents are on disk, the answer is instant,
    and it is the same list ``collect`` would report — ``collect``'s extra work is establishing
    that the *methodologies* are authentic, which is a different question from which files they
    name.
    """
    import yaml

    root = Path(root)
    directory = root / "validation"
    if not directory.is_dir():
        return []
    found = set()
    for document in sorted(directory.glob("*.yaml")):
        try:
            doc = yaml.safe_load(document.read_text(encoding="utf-8"))
        except Exception:
            continue
        if not isinstance(doc, dict):
            continue
        for entry in doc.get("data") or []:
            for item in (entry or {}).get("files") or []:
                path = (item or {}).get("path")
                if path and (root / path).is_file():
                    found.add(str(path))
    return sorted(found)


# -- versions --------------------------------------------------------------------------------

#: `v0.0.0`, the shape both manifests use.
_VERSION = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)$")


def parse_version(text):
    """``v1.2.3`` into ``(1, 2, 3)``, or ``None`` for anything this tool should not rewrite."""
    match = _VERSION.match(str(text or "").strip())
    return tuple(int(part) for part in match.groups()) if match else None


def format_version(parts, like="v0.0.0"):
    """``(1, 2, 3)`` back into a string, keeping whatever ``v`` prefix the manifest already used."""
    prefix = "v" if str(like).strip().startswith("v") else ""
    return prefix + ".".join(str(part) for part in parts)


#: Functional files changed and the observed behaviour did not: code in a formats repository whose
#: expected test outputs are unchanged, or a new grant.
PATCH = "patch"

#: New capability, every earlier output unchanged: a format test added, a list extended.
MINOR = "minor"

#: Earlier outputs change: an expected test output edited or removed, a company's matching changed.
MAJOR = "major"

_STRENGTH = {PATCH: 0, MINOR: 1, MAJOR: 2}

#: An input database's bump is fixed per directory. `companies/` decides what *matches*, so a
#: change there changes the meaning of every run. `lists/` is minor because the typical list change
#: adds companies -- rows are added, none changed -- and a fingerprint cannot tell an addition from
#: a removal, so a person who removed companies raises it to major by hand.
INPUT_DB_KINDS = {"companies": MAJOR, "lists": MINOR}


def bump(version, kind):
    """The proposed next version for one kind of change."""
    major, minor, patch = version
    if kind == MAJOR:
        return (major + 1, 0, 0)
    if kind == MINOR:
        return (major, minor + 1, 0)
    if kind == PATCH:
        return (major, minor, patch + 1)
    return version


def strongest(kinds):
    """The kind a set of changes calls for together, or ``None`` when one of them is unknown.

    One change this tool cannot classify is enough to leave the whole choice to a person: proposing
    the strongest of the *others* would understate it.
    """
    kinds = list(kinds)
    if not kinds or any(kind is None for kind in kinds):
        return None
    return max(kinds, key=_STRENGTH.__getitem__)


# -- the rule, per repository kind ------------------------------------------------------------


class FingerprintCheck:
    """One repository's fingerprints: what they say, what they should say, and what that demands.

    ``committed`` is the value at HEAD rather than the one in the working tree, and that is the
    whole subtlety of the rule: the hook rewrites the manifest itself, so comparing against the
    working tree would compare a value with the value it was just given and always agree.
    """

    def __init__(self, root, entries, version_field, version_now, version_committed):
        self.root = root
        self.entries = entries
        self.version_field = version_field
        self.version_now = version_now
        self.version_committed = version_committed

    @property
    def moved(self):
        """The fingerprints that differ from what the commit at HEAD declared."""
        return [
            entry for entry in self.entries if entry["computed"] != entry["committed"]
        ]

    @property
    def version_moved(self):
        """Whether the declared version already differs from HEAD's.

        A repository with no HEAD -- nothing committed yet -- has no version to have moved away
        from, and is treated as though it had: there is no earlier claim for this one to contradict.
        """
        if self.version_committed is None:
            return True
        return self.version_now != self.version_committed

    @property
    def ok(self):
        """Whether the manifest's claim about its own contents is currently true."""
        return not self.moved or self.version_moved

    @property
    def changed_directories(self):
        return {entry["name"] for entry in self.moved}

    def proposal(self):
        """The version to move to: the strongest kind among the fingerprints that moved.

        ``None`` where there is nothing to propose: nothing moved, the version already moved, a
        change this tool cannot classify, or a version that is not a shape this tool should
        rewrite. A version somebody wrote as ``2024.3`` is left alone rather than guessed at.
        """
        if not self.moved or self.version_moved:
            return None
        kind = strongest(entry["kind"] for entry in self.moved)
        current = parse_version(self.version_now)
        if kind is None or current is None:
            return None
        return kind, format_version(bump(current, kind), self.version_now)

    @property
    def stale(self):
        """Whether what is written down -- manifest fields, saved listings -- differs from now."""
        for entry in self.entries:
            if entry["declared"] != entry["computed"]:
                return True
            if entry.get("listing") is not None:
                saved = Path(self.root) / entry["listing_path"]
                if (
                    not saved.is_file()
                    or saved.read_text(encoding="utf-8") != entry["listing"]
                ):
                    return True
        return False

    def record(self, manifest):
        """Write every computed fingerprint, and any listing that goes with one, then the manifest.

        Only to be called when the manifest is entitled to the values -- the version moved, or a
        proposal was accepted -- which is the caller's rule to apply, not this method's.
        """
        for entry in self.entries:
            if entry["declared"] != entry["computed"]:
                manifest.set(entry["keys"], entry["computed"])
            if entry.get("listing") is not None:
                (Path(self.root) / entry["listing_path"]).write_text(
                    entry["listing"], encoding="utf-8"
                )
        manifest.write()


def committed_text(root, relative_path):
    """A file as the commit at HEAD has it, or ``None`` when there is no such commit or file."""
    import subprocess

    try:
        done = subprocess.run(
            ["git", "-C", str(root), "show", f"HEAD:{relative_path}"],
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError:
        return None
    return done.stdout if done.returncode == 0 else None


def committed_manifest(root, relative_path):
    """The manifest as the commit at HEAD has it, or ``None`` when there is no such commit.

    ``None`` is not a failure. A repository with no commits, or one where the manifest is not yet
    tracked, has made no earlier claim for the current one to contradict.
    """
    text = committed_text(root, relative_path)
    if text is None:
        return None
    import yaml

    try:
        doc = yaml.safe_load(text)
    except Exception:
        return None
    return doc if isinstance(doc, dict) else None


def _committed(doc, *keys):
    node = doc
    for key in keys:
        if not isinstance(node, dict) or key not in node:
            return None
        node = node[key]
    return node


#: The saved listing of the expected test outputs, beside the directory it describes.
TESTS_LISTING = "tests/formats.sha256"


def check_formats(root):
    """A formats repository: three identity fingerprints and the certification one.

    ``content/`` and ``metadata/`` say *how* documents are read; ``tests/formats/`` holds what
    reading them is expected to produce, and is taken as the behavioural specification. So a code
    change whose expected outputs are unchanged is a patch, and the expected outputs alone decide
    between minor and major -- which needs to know whether files were only *added*, and that is why
    their listing is saved in the repository (:data:`TESTS_LISTING`) rather than only its hash.

    ``validation_sha256`` covers every granted file. It certifies rather than identifies, and a
    new grant changes no output: a patch.
    """
    from freeports_dev.ci.manifest import Manifest

    root = Path(root)
    manifest = Manifest(root / "package.yaml")
    committed = committed_manifest(root, "package.yaml") or {}

    def entry(name, keys, computed, kind, **extra):
        return {
            "name": name,
            "keys": keys,
            "computed": computed,
            "declared": manifest.get(*keys),
            "committed": _committed(committed, *keys),
            "kind": kind,
            **extra,
        }

    tests_listing = listing(root, files_under(root, "tests/formats"))
    tests_before = committed_text(root, TESTS_LISTING)
    entries = [
        entry(name, ("sha256", name), fingerprint(root, files_under(root, name)), PATCH)
        for name in ("content", "metadata")
    ]
    entries.append(
        entry(
            "tests",
            ("sha256", "tests"),
            hash_text(tests_listing),
            listing_kind(tests_before, tests_listing),
            listing=tests_listing,
            listing_path=TESTS_LISTING,
            changes=listing_changes(tests_before, tests_listing)
            if tests_before is not None
            else None,
        )
    )
    entries.append(
        entry(
            "validation",
            ("info", "validation_sha256"),
            fingerprint(root, granted_files(root)),
            PATCH,
        )
    )
    return manifest, FingerprintCheck(
        root,
        entries,
        ("info", "version"),
        manifest.get("info", "version"),
        _committed(committed, "info", "version"),
    )


def check_input_db(root):
    """An input database: two fingerprints, over ``companies/`` and over ``lists/``."""
    from freeports_dev.ci.manifest import Manifest

    root = Path(root)
    manifest = Manifest(root / "metadata.yaml")
    committed = committed_manifest(root, "metadata.yaml")
    entries = []
    for name in ("companies", "lists"):
        entries.append(
            {
                "name": name,
                "keys": ("sha256", name),
                "computed": fingerprint(root, files_under(root, name)),
                "declared": manifest.get("sha256", name),
                "committed": _committed(committed or {}, "sha256", name),
                "kind": INPUT_DB_KINDS[name],
            }
        )
    return manifest, FingerprintCheck(
        root,
        entries,
        ("info", "version"),
        manifest.get("info", "version"),
        _committed(committed or {}, "info", "version"),
    )


def check(root, repo_kind):
    """The rule for whichever kind of repository this is, or ``(None, None)`` where there is none.

    The engine repository declares no fingerprint of its own and needs none: it has no manifest,
    and what it ships is versioned by its packages.
    """
    from freeports_dev.ci import metrics

    if repo_kind == metrics.FORMATS:
        return check_formats(root)
    if repo_kind == metrics.INPUT_DB:
        return check_input_db(root)
    return None, None
