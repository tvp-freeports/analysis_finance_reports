#!/usr/bin/env python3
"""The pattern grammar of a `Supported paths` section, and what it matches in a repository.

These patterns are read by people deciding whether to trust a grant, so the grammar is deliberately
tiny -- four rules, and everything else is a literal character:

==================  ==========================================================================
``*``               any part of one path segment; never crosses a ``/``
``**``              zero or more whole segments
a trailing ``/``    the directory and everything under it -- the same as appending ``**``
anything else       itself
==================  ==========================================================================

There are no character classes, no braces, no negation and no ``?``. A pattern that looks like a
shell glob but is not one would be worse than no pattern language at all: the reader would believe
something the tool does not do.

Like `rst_paths.py`, this is a script rather than a module, and for the same reason -- ``**`` against
a path is the sort of `awk` nobody can check by reading. Three ways to run it::

    $ python3 pathmatch.py match 'tests/formats/*/*/out/*.csv' tests/formats/FOO-EN24/1/out/funds.csv
    $ echo 'tests/formats/**' | python3 pathmatch.py filter a/b.csv c/d.csv
    $ python3 pathmatch.py expand 'tests/formats/*/*/out/*.csv' /path/to/repository

`match` is the one to retype at a prompt: it prints nothing and answers in its exit status, like
`test`. `filter` is the one the shell scripts call, because a document may grant hundreds of files
under one methodology and starting an interpreter per file to answer a yes-or-no question is a cost
with nothing behind it. `expand` is the coverage denominator: what a pattern *could* cover in a
repository as it stands.
"""

import json
import os
import re
import sys
from functools import lru_cache


#: Never part of what a methodology covers, and never worth walking. A repository's own Git
#: metadata is not a file anybody grants, and it is large enough that including it would make
#: `expand` noticeably slower for an answer that is always the same.
UNWALKED = {".git"}


def pattern_segments(pattern):
    """A pattern as the list of segments the matcher walks.

    A trailing ``/`` becomes an explicit ``**``, which is what makes "the directory and everything
    under it" the same rule as the one above it rather than a special case in the matcher. Empty
    segments -- a leading slash, or a doubled one -- are dropped: patterns are relative to the
    repository root, and a slash in front of one says the same thing it already said.
    """
    text = pattern.strip()
    trailing = text.endswith("/")
    segments = [segment for segment in text.split("/") if segment]
    if trailing:
        segments.append("**")
    return segments


@lru_cache(maxsize=None)
def _segment_matcher(segment):
    """One segment as a regular expression, with ``*`` standing for a non-empty run of non-slash.

    Non-empty on purpose: ``*.csv`` is a name with an extension, and reading it as one that also
    matches a bare ``.csv`` would quietly widen every pattern anybody writes.
    """
    return re.compile(
        "^" + "[^/]+".join(re.escape(part) for part in segment.split("*")) + "$"
    )


def _match_segments(pattern, path):
    """Match two segment lists, with ``**`` standing for zero or more of them.

    Written as a recursion over the two lists rather than as one compiled regular expression,
    because the definition of ``**`` *is* "try it against zero segments, then one, then two", and a
    reader checking whether the tool agrees with the documentation can see that here.
    """
    if not pattern:
        return not path
    head, tail = pattern[0], pattern[1:]
    if head == "**":
        return any(
            _match_segments(tail, path[taken:]) for taken in range(len(path) + 1)
        )
    if not path:
        return False
    if not _segment_matcher(head).match(path[0]):
        return False
    return _match_segments(tail, path[1:])


def matches(pattern, path):
    """True when a repository-relative path is one the pattern covers."""
    return _match_segments(
        pattern_segments(pattern), [segment for segment in path.split("/") if segment]
    )


def expand(pattern, root):
    """Every file under ``root`` the pattern covers, as sorted repository-relative paths.

    Sorted because this is a denominator: a coverage figure that depended on the order the
    filesystem happened to hand its entries back would differ between two runs over one tree.
    """
    found = []
    for directory, subdirectories, filenames in os.walk(root):
        subdirectories[:] = [name for name in subdirectories if name not in UNWALKED]
        for filename in filenames:
            relative = os.path.relpath(os.path.join(directory, filename), root)
            if matches(pattern, relative):
                found.append(relative)
    return sorted(found)


def _patterns_from_stdin():
    return [line.strip() for line in sys.stdin.read().splitlines() if line.strip()]


def _filter(paths):
    """For each path, the first pattern from standard input that covers it, or ``None``.

    The *first*, not all of them, because the answer's purpose is to be shown to a person: a path is
    in scope or it is not, and if it is, the pattern that put it there is the one whose prose
    explains what vouching for it means.
    """
    patterns = _patterns_from_stdin()
    answer = []
    for path in paths:
        matched = next(
            (pattern for pattern in patterns if matches(pattern, path)), None
        )
        answer.append({"path": path, "matched": matched})
    return answer


def main(argv):
    if not argv:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    mode, arguments = argv[0], argv[1:]

    if mode == "match":
        if len(arguments) != 2:
            print("Usage: pathmatch.py match <pattern> <path>", file=sys.stderr)
            return 2
        return 0 if matches(arguments[0], arguments[1]) else 1

    if mode == "filter":
        if not arguments:
            print(
                "Usage: pathmatch.py filter <path>...   (patterns on standard input)",
                file=sys.stderr,
            )
            return 2
        json.dump(_filter(arguments), sys.stdout, indent=2)
        sys.stdout.write("\n")
        return 0

    if mode == "expand":
        if len(arguments) != 2:
            print("Usage: pathmatch.py expand <pattern> <root>", file=sys.stderr)
            return 2
        json.dump(expand(arguments[0], arguments[1]), sys.stdout, indent=2)
        sys.stdout.write("\n")
        return 0

    print(f"Unknown mode: {mode}", file=sys.stderr)
    print("Modes: match, filter, expand", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
