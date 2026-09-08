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


def fingerprint(root, relative_paths):
    """The hash of the hashes, over a set of paths relative to ``root``.

    Sorted with :func:`sorted` over the raw strings, which is Python's byte-order comparison and
    therefore the same order ``LC_ALL=C sort`` produces. Naming the locale in the shell one-liner
    and relying on the default here would be two recipes that agree until somebody's machine
    disagrees.
    """
    root = Path(root)
    stream = hashlib.sha256()
    for relative in sorted(str(p) for p in relative_paths):
        stream.update(
            _LINE.format(hash=hash_file(root / relative), path=relative).encode("utf-8")
        )
    return stream.hexdigest()


def files_under(root, directory):
    """Every file under one directory of the repository, as paths relative to the root.

    Directories are not hashed and empty ones therefore do not exist as far as a fingerprint is
    concerned, which matches what git records and means the fingerprint does not move for a
    difference git will not carry anyway.
    """
    base = Path(root) / directory
    if not base.is_dir():
        return []
    return sorted(
        str(path.relative_to(root)) for path in base.rglob("*") if path.is_file()
    )


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


#: A change under `lists/` alters only which companies a list associates; the meaning of a match is
#: unchanged, so the minor component moves.
MINOR = "minor"

#: A change under `companies/` changes what *matches*, and therefore the meaning of every run. The
#: major component moves and the minor resets. Both directories changed is still a major bump,
#: because a companies change subsumes a lists one.
MAJOR = "major"


def bump(version, kind):
    """The proposed next version, by the rule the two directories imply.

    The patch component is deliberately never proposed: it is what a change that moves neither
    fingerprint gets — a description, a README — and only a person knows they made one.
    """
    major, minor, patch = version
    if kind == MAJOR:
        return (major + 1, 0, 0)
    if kind == MINOR:
        return (major, minor + 1, 0)
    return version


def bump_kind(changed):
    """Which component moves, given the set of directories whose fingerprint moved.

    ``companies`` wins whenever it is present: it decides what matches, while ``lists`` decides
    only the association, so a change to both is a companies change with a lists change inside it
    rather than two changes of equal weight.
    """
    if "companies" in changed:
        return MAJOR
    if "lists" in changed:
        return MINOR
    return None


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
        """The version to move to, for an input database where the bump is mechanical.

        ``None`` where there is nothing to propose: nothing moved, the version already moved, or
        the version is not a shape this tool should rewrite. A version somebody wrote as ``2024.3``
        is a version this tool leaves alone rather than guesses at.
        """
        if not self.moved or self.version_moved:
            return None
        kind = bump_kind(self.changed_directories)
        current = parse_version(self.version_now)
        if kind is None or current is None:
            return None
        return kind, format_version(bump(current, kind), self.version_now)


def committed_manifest(root, relative_path):
    """The manifest as the commit at HEAD has it, or ``None`` when there is no such commit.

    ``None`` is not a failure. A repository with no commits, or one where the manifest is not yet
    tracked, has made no earlier claim for the current one to contradict.
    """
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
    if done.returncode != 0:
        return None
    import yaml

    try:
        doc = yaml.safe_load(done.stdout)
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


def check_formats(root):
    """A formats repository: one fingerprint, over every file a grant covers."""
    from freeports_dev.ci.manifest import Manifest

    root = Path(root)
    manifest = Manifest(root / "package.yaml")
    committed = committed_manifest(root, "package.yaml")
    return manifest, FingerprintCheck(
        root,
        [
            {
                "name": "validation",
                "keys": ("info", "validation_sha256"),
                "computed": fingerprint(root, granted_files(root)),
                "declared": manifest.get("info", "validation_sha256"),
                "committed": _committed(committed or {}, "info", "validation_sha256"),
            }
        ],
        ("info", "version"),
        manifest.get("info", "version"),
        _committed(committed or {}, "info", "version"),
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
