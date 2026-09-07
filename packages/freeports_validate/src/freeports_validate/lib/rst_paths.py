#!/usr/bin/env python3
"""What a methodology page declares about itself, read out of its reStructuredText.

Everything else in this command is shell, on purpose: a hash, a signature, a fetch and a comparison
are all commands a user can retype at a prompt, and being able to retype them is how the mechanism
gets understood rather than trusted. Parsing reStructuredText is the opposite kind of work. Done in
`awk` it would be the sort of cleverness nobody can verify by reading, which would defeat the very
property the shell is there for -- so it is Python, and the rule the split follows is that **bash
establishes facts and Python parses and renders**.

It is a script rather than a module: it reads a page on standard input and writes JSON on standard
output, so it can be run by hand exactly the way `lib/paths.sh` and `lib/links.sh` run it::

    $ python3 rst_paths.py < methodologies/basic_check.rst
    {
      "supported_paths": {"declared": true, "patterns": [{"pattern": "...", "prose": "..."}]},
      "links": {"pinned": [{"target": "...", "sha256": "..."}], "unpinned": [], "stale": []}
    }

    $ python3 rst_paths.py pin new-hashes.txt < page.rst > page.rst.new

One call answers about both things a page says about itself, because both come out of one reading of
it and a caller that wanted both should not have to parse it twice.

The section it looks for is an ordinary, *visible* section titled ``Supported paths``, whose entries
are a definition list -- a term that is a single inline literal, and an indented gloss under it::

    Supported paths
    ===============

    Vouching for a file under this methodology means the protocol above was applied to it. What
    that means concretely depends on what the file is, so each path this methodology covers says it:

    ``tests/formats/*/*/out/*.csv``
        The reference output of a format's test suite in a **formats repository**.

Nothing here is a directive or a comment. A hidden comment block would put the declaration where a
reader of the published page could not see it, and a custom Sphinx directive would break every
plain-reStructuredText consumer -- including this parser's own reason for existing, which is that the
page is fetched as text from wherever its author published it.

Two absences mean different things, and the JSON keeps them apart. ``declared: false`` is a page with
no such section at all, and means the methodology supports **any** path -- the behaviour before this
section existed, and the honest answer for a methodology whose scope cannot be written as a set of
paths. ``declared: true`` with no patterns is a page that opened the subject and named nothing, which
is a page to fix rather than a licence to grant anywhere.

The second half is the page's **sub-hashes**. A methodology page cites things -- a diagram, a
specification, another document -- and a grant made under it is in part a claim about what those said,
so the page may pin each of them in an ordinary comment::

    .. image:: assets/pipeline.svg

    .. sha256: assets/pipeline.svg 3f2a1b...c9

Those comment lines are part of the page, so the page's own hash already commits to every one of
them: the pins are a *deepening* of the page's hash, never a second thing to trust. What this parser
reports is an inventory -- which cited targets are pinned, which are not, and which pins name
something the page no longer cites at all -- and `pin` rewrites that inventory in place once
somebody has fetched the targets and hashed them.
"""

import json
import re
import sys
import textwrap


#: The title that turns a section into a declaration. Compared case-insensitively: `Supported Paths`
#: is unmistakably the same section, and the failure mode of being strict is the worst of the three
#: available -- the page would constrain nothing, and say so nowhere.
SECTION_TITLE = "supported paths"

#: The characters reStructuredText allows to underline a title. Any of them, repeated.
ADORNMENT_CHARACTERS = set("!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~")

#: A definition-list term that declares a pattern: one inline literal, alone on its line, at the
#: left margin. Alone, because a literal inside a sentence is prose about a pattern rather than a
#: declaration of one, and the difference is exactly the difference between a page discussing paths
#: and a page committing to them.
TERM = re.compile(r"^``([^`]+)``\s*$")


def _is_adornment(line):
    """True for a line that is nothing but one punctuation character repeated.

    Two characters minimum: a single ``-`` at the left margin is a bullet far more often than it is
    the underline of a one-letter title.
    """
    text = line.rstrip()
    return (
        len(text) >= 2
        and text[0] in ADORNMENT_CHARACTERS
        and set(text) == {text[0]}
        and not line[:1].isspace()
    )


def _sections(lines):
    """Every section in the document, as ``(title, style, body_start, title_start)``.

    ``style`` is what reStructuredText decides levels by: the adornment character, and whether the
    title is overlined as well as underlined. The level of a style is not written anywhere in the
    page -- it is the order in which the styles first appear -- which is why the levels are worked
    out in :func:`_levels` over the whole document rather than guessed at one heading.

    A title has to sit at the left margin. Prose indented under a definition-list term can look
    exactly like a title otherwise, and a gloss ending in a row of dashes would silently close the
    section it belongs to.
    """
    found = []
    index = 0
    while index < len(lines):
        line = lines[index]
        overlined = (
            _is_adornment(line)
            and index + 2 < len(lines)
            and lines[index + 1].strip()
            and not lines[index + 1][:1].isspace()
            and _is_adornment(lines[index + 2])
            and lines[index + 2].rstrip()[0] == line.rstrip()[0]
            and len(line.rstrip()) >= len(lines[index + 1].rstrip())
            and len(lines[index + 2].rstrip()) >= len(lines[index + 1].rstrip())
        )
        if overlined:
            found.append(
                (
                    lines[index + 1].strip(),
                    (lines[index + 2].rstrip()[0], True),
                    index + 3,
                    index,
                )
            )
            index += 3
            continue

        underlined = (
            line.strip()
            and not line[:1].isspace()
            and not _is_adornment(line)
            and index + 1 < len(lines)
            and _is_adornment(lines[index + 1])
            and len(lines[index + 1].rstrip()) >= len(line.rstrip())
        )
        if underlined:
            found.append(
                (
                    line.strip(),
                    (lines[index + 1].rstrip()[0], False),
                    index + 2,
                    index,
                )
            )
            index += 2
            continue

        index += 1
    return found


def _levels(sections):
    """The nesting level of each section: the order in which its style first appeared."""
    order = []
    levels = []
    for _title, style, _body_start, _title_start in sections:
        if style not in order:
            order.append(style)
        levels.append(order.index(style))
    return levels


def _section_body(lines, sections, levels, position):
    """The lines belonging to one section: down to the next heading that is not below it."""
    body_start = sections[position][2]
    for later in range(position + 1, len(sections)):
        if levels[later] <= levels[position]:
            return lines[body_start : sections[later][3]]
    return lines[body_start:]


def _entries(body):
    """The definition-list entries of a section body, as ``{pattern, prose}``.

    Anything that is not a term with a block under it is ordinary prose and is passed over. That is
    what lets the section read as a section: an author introduces the list in a sentence, groups the
    entries under subheadings, and explains between them, and none of it changes what is declared.
    """
    entries = []
    index = 0
    while index < len(body):
        term = TERM.match(body[index])
        if term is None:
            index += 1
            continue
        block = []
        index += 1
        while index < len(body) and (
            not body[index].strip() or body[index][:1].isspace()
        ):
            block.append(body[index])
            index += 1
        entries.append(
            {
                "pattern": term.group(1).strip(),
                "prose": textwrap.dedent("\n".join(block)).strip(),
            }
        )
    return entries


def supported_paths(text):
    """The ``Supported paths`` declaration of a page, as ``{declared, patterns}``."""
    lines = text.splitlines()
    sections = _sections(lines)
    levels = _levels(sections)
    for position, (title, _style, _body_start, _title_start) in enumerate(sections):
        if title.casefold() != SECTION_TITLE:
            continue
        body = _section_body(lines, sections, levels, position)
        return {"declared": True, "patterns": _entries(body)}
    return {"declared": False, "patterns": []}


# ---------------------------------------------------------------------------
# Sub-hashes: what the page cites, and what it pins
# ---------------------------------------------------------------------------

#: A pin. The target is named in the comment rather than inferred from what the line sits next to,
#: so adjacency is for the reader and the parser never has to guess which link a comment belongs to.
#: The hash is required to be a full sha256: a truncated one is not a pin, because comparing against
#: half a claim would report agreement or disagreement about something nobody actually asserted.
PIN = re.compile(r"^\s*\.\.\s+sha256:\s*(\S+)\s+([0-9a-fA-F]{64})\s*$")

#: An image or figure. Its argument is the one target that is routinely a path relative to the page
#: rather than a URI, which is why it is matched on its own and not left to `URI` below.
IMAGE = re.compile(r"^\s*\.\.\s+(?:image|figure)::\s*(\S+)\s*$", re.MULTILINE)

#: Any absolute URI in the page's visible text. One expression covers all three ways to write a
#: link -- inline ``\`text <URI>\`_``, a target definition ``.. _name: URI``, and a bare URI, which
#: docutils turns into a link too -- because what makes each of them a target is the URI itself and
#: not the syntax around it. ``<`` and ``>`` are excluded from the body, which is what makes the
#: inline form terminate correctly.
URI = re.compile(r"[a-zA-Z][a-zA-Z0-9+.\-]*://[^\s<>`\"'\\]+")

#: Trailing punctuation a sentence puts after a URI rather than inside it. A URI genuinely ending in
#: one of these has to be written in the inline form, where ``>`` ends it unambiguously -- which is
#: the form an author who cares about the exact bytes should be using in any case.
URI_TRAILERS = ".,;:!?)]}>'\""

#: Directives whose indented body is code rather than prose. Everything else -- `note`, `warning`,
#: `tip` -- has prose inside it, and prose has real links in it, so only these are skipped.
LITERAL_DIRECTIVE = re.compile(
    r"^\s*\.\.\s+(?:code-block|code|parsed-literal|literalinclude|highlight)::"
)

DIRECTIVE = re.compile(r"^\s*\.\.\s")


def _literal_line_numbers(lines):
    """The lines that are an example rather than prose, and whose links are therefore not links.

    Two things open one: a paragraph ending in ``::``, which is reStructuredText's own literal block,
    and a directive from :data:`LITERAL_DIRECTIVE`. An admonition also ends in ``::`` but is a
    directive with prose inside it, so the two cases are told apart rather than lumped together --
    treating `.. note::` as literal would drop every link a page put in one.
    """
    literal = set()
    index = 0
    while index < len(lines):
        line = lines[index]
        stripped = line.strip()
        if not stripped:
            index += 1
            continue
        if DIRECTIVE.match(line):
            opens = bool(LITERAL_DIRECTIVE.match(line))
        else:
            opens = stripped.endswith("::")
        if not opens:
            index += 1
            continue

        margin = len(line) - len(line.lstrip())
        index += 1
        while index < len(lines):
            following = lines[index]
            if not following.strip():
                literal.add(index)
                index += 1
                continue
            if len(following) - len(following.lstrip()) <= margin:
                break
            literal.add(index)
            index += 1
    return literal


def _visible(lines):
    """The page with its examples and its own pins blanked out, line numbering intact.

    Blanking the pins matters: a pin is the only place a withdrawn target may still be named, and
    reading that mention as a citation would let a stale pin keep itself alive for ever.
    """
    literal = _literal_line_numbers(lines)
    return [
        "" if index in literal or PIN.match(line) else line
        for index, line in enumerate(lines)
    ]


def _targets(lines):
    """Everything the page cites, in the order it cites it, each named once.

    Order is the page's rather than sorted, because the inventory is read next to the page it
    describes, and a reader checking one against the other reads both downwards.
    """
    visible = "\n".join(_visible(lines))
    found = []
    for match in IMAGE.finditer(visible):
        found.append((match.start(1), match.group(1)))
    for match in URI.finditer(visible):
        found.append((match.start(), match.group().rstrip(URI_TRAILERS)))

    ordered = []
    for _position, target in sorted(found):
        if target not in ordered:
            ordered.append(target)
    return ordered


def _pins(lines):
    """The pins the page declares, as target -> hash, in the order they appear."""
    pins = {}
    for line in lines:
        match = PIN.match(line)
        if match is not None:
            pins[match.group(1)] = match.group(2).lower()
    return pins


def links(text):
    """The page's sub-hash inventory: ``{pinned, unpinned, stale}``.

    ``stale`` is a pin naming something the page no longer cites. It is reported rather than
    quietly dropped because it is a claim the page is still making about a resource it has stopped
    depending on, and the person who removed the citation is the one who should say whether the
    claim goes with it.
    """
    lines = text.splitlines()
    targets = _targets(lines)
    pins = _pins(lines)
    return {
        "pinned": [
            {"target": target, "sha256": pins[target]}
            for target in targets
            if target in pins
        ],
        "unpinned": [target for target in targets if target not in pins],
        "stale": [
            {"target": target, "sha256": digest}
            for target, digest in pins.items()
            if target not in targets
        ],
    }


def _pin_line(target, digest):
    return f".. sha256: {target} {digest}"


def _end_of_block(lines, start):
    """The blank line that closes the block ``start`` belongs to, or the end of the page.

    A pin is written after the whole block and not after the line, so that an image directive keeps
    its options -- ``:width:``, ``:alt:`` -- and the comment lands under the directive rather than
    inside it, where reStructuredText would read it as one of those options.
    """
    index = start + 1
    while index < len(lines) and lines[index].strip():
        index += 1
    return index


def pin(text, hashes):
    """``text`` with its pins brought into line with ``hashes`` (target -> hash).

    Three things happen, and the third is the one worth stating: a pin whose target the page still
    cites but which ``hashes`` says nothing about is **kept exactly as it was**. That is the case of
    a target that could not be fetched, and rewriting it -- or dropping it -- would turn a network
    failure into a silently withdrawn claim.

    Idempotent by construction: an existing pin is rewritten where it stands rather than moved, so a
    second run over the same page and the same hashes produces the same bytes.
    """
    lines = text.splitlines()
    cited = set(_targets(lines))

    kept = []
    seen = set()
    for line in lines:
        match = PIN.match(line)
        if match is None:
            kept.append(line)
            continue
        target = match.group(1)
        if target not in cited:
            continue  # stale: the page stopped citing it
        seen.add(target)
        kept.append(_pin_line(target, hashes.get(target, match.group(2).lower())))

    # Written back to front, so that an insertion never moves the line a later one was measured
    # against.
    insertions = []
    for target, digest in hashes.items():
        if target in seen or target not in cited:
            continue
        for number, line in enumerate(_visible(kept)):
            if target in line:
                insertions.append((_end_of_block(kept, number), target, digest))
                break

    for position, target, digest in sorted(insertions, reverse=True):
        kept[position:position] = ["", _pin_line(target, digest)]

    return "\n".join(kept) + "\n"


def _read_hashes(path):
    """``target hash`` lines, as the shell wrote them after fetching each target."""
    hashes = {}
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            parts = line.split()
            if len(parts) == 2:
                hashes[parts[0]] = parts[1].lower()
    return hashes


def main(argv):
    if argv and argv[0] == "pin":
        if len(argv) != 2:
            print(
                "Usage: rst_paths.py pin <hashes-file>   (page on stdin)",
                file=sys.stderr,
            )
            return 2
        sys.stdout.write(pin(sys.stdin.read(), _read_hashes(argv[1])))
        return 0

    if argv and argv[0] not in ("report",):
        print(f"Unknown mode: {argv[0]}", file=sys.stderr)
        print("Modes: report (the default), pin", file=sys.stderr)
        return 2

    text = sys.stdin.read()
    json.dump(
        {"supported_paths": supported_paths(text), "links": links(text)},
        sys.stdout,
        indent=2,
    )
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
