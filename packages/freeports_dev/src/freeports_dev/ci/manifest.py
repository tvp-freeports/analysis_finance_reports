"""Reading and rewriting the two manifests, without reformatting what nobody asked to change.

``package.yaml`` and ``metadata.yaml`` are files people wrote and people read. Round-tripping them
through a YAML dumper would reflow the quoting, drop every comment and reorder nothing visibly but
enough to make the diff of a one-field change unreadable — and a commit hook that produces an
unreadable diff is a commit hook people stop trusting.

So a field is replaced **in place, by line**, and everything else in the file is left exactly as it
was. The cost is that this only handles the shape these two manifests actually have: a top-level
key, or one level of nesting under it. That is not a limitation worth engineering around, because
the alternative — a general YAML round-trip — is what produces the diffs nobody can read.

The file is still *parsed* with the YAML reader to find out what a field currently says. Parsing is
safe; it is only writing that has to be careful.
"""

import re
from pathlib import Path


class ManifestError(Exception):
    """A manifest that is not there, or does not hold the field this rule is about."""


class Manifest:
    """One ``package.yaml`` or ``metadata.yaml``, read for its fields and edited by line."""

    def __init__(self, path):
        self.path = Path(path)
        if not self.path.exists():
            raise ManifestError(f"{self.path} is not there")
        self.text = self.path.read_text(encoding="utf-8")
        import yaml

        try:
            self.doc = yaml.safe_load(self.text) or {}
        except Exception as exc:
            raise ManifestError(f"{self.path} does not parse: {exc}") from exc
        if not isinstance(self.doc, dict):
            raise ManifestError(f"{self.path} must hold a mapping")

    def get(self, *keys):
        """A nested field's current value, or ``None`` when the path is not there."""
        node = self.doc
        for key in keys:
            if not isinstance(node, dict) or key not in node:
                return None
            node = node[key]
        return node

    def set(self, keys, value):
        """Replace one field's value in place, keeping every other byte of the file.

        Matched by indentation as well as by name, so that a ``version:`` under ``info:`` is not
        confused with a top-level one — which is exactly the pair these two manifests contain.
        """
        keys = list(keys)
        if not keys:
            raise ManifestError("no field named")
        if self.get(*keys) is None:
            raise ManifestError(
                f"{self.path} has no {'.'.join(keys)} to update. "
                f"Add the field by hand once; this rule will keep it current after that."
            )

        lines = self.text.splitlines(keepends=True)
        parents, leaf = keys[:-1], keys[-1]
        depth = 0
        index = 0

        for parent in parents:
            pattern = re.compile(rf"^(\s{{{depth * 2}}}){re.escape(parent)}\s*:")
            while index < len(lines) and not pattern.match(lines[index]):
                index += 1
            if index >= len(lines):
                raise ManifestError(f"{self.path}: could not find {parent}:")
            index += 1
            depth += 1

        pattern = re.compile(rf"^(\s*){re.escape(leaf)}(\s*:)(.*)$")
        while index < len(lines):
            match = pattern.match(lines[index])
            if match:
                indent = match.group(1)
                # A field one level too shallow belongs to the next section, not to this one.
                if parents and len(indent) < depth * 2:
                    break
                ending = "\n" if lines[index].endswith("\n") else ""
                lines[index] = f"{indent}{leaf}: {value}{ending}"
                self.text = "".join(lines)
                self._reparse()
                return
            index += 1
        raise ManifestError(f"{self.path}: could not find {'.'.join(keys)} to rewrite")

    def _reparse(self):
        import yaml

        self.doc = yaml.safe_load(self.text) or {}

    def write(self):
        self.path.write_text(self.text, encoding="utf-8")
        return self.path
