"""Ready-made probes: small scripts that ask one question of a document.

A probe is what one writes before a format exists -- *what kind of document is this*, *what sits
under the management company label* -- to find out whether an assumption holds on more than one
report. The ones general enough to be useful to the next format author ship in ``probes/`` next to
this module; directories of one's own, named with ``dev.probes_dirs``, are searched first.

Two decisions are held here.

**The tool never imports a probe.** Describing one reads its docstring with :mod:`ast`; running one
starts it as a separate process with the tool's own interpreter. So a probe is an independent file
with no interface to honour beyond "a docstring, arguments first, documents last", and a broken probe
breaks only itself.

**A substitution is always visible.** A probe of one's own with the name of a shipped one takes its
place -- that is how a probe is improved before it is published -- and the listing says so.
"""

import ast
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

#: The probes that ship with the tool.
SHIPPED_DIR = Path(__file__).resolve().parent / "probes"

PREFIX = "probe_"


class ProbeError(Exception):
    """A probe that cannot be found or a command line that cannot be split."""


@dataclass(frozen=True)
class Probe:
    name: str
    path: Path
    origin: Path
    summary: str
    #: The probe of the same name this one stands in for, if any.
    hides: Path = None


def summary_of(path):
    """The first line of a probe's docstring, read without executing the file."""
    try:
        tree = ast.parse(Path(path).read_text(encoding="utf-8"))
    except (SyntaxError, UnicodeDecodeError, OSError) as error:
        return f"(unreadable: {error.__class__.__name__})"
    docstring = ast.get_docstring(tree)
    if not docstring or not docstring.strip():
        return "(no description)"
    return docstring.strip().splitlines()[0].strip()


def _probes_in(directory):
    return sorted(Path(directory).glob(f"{PREFIX}*.py"))


def discover(own_dirs):
    """Every probe by name: one's own directories first, in the order given, then the shipped ones."""
    probes = {}
    for directory in own_dirs:
        directory = Path(directory)
        if not directory.is_dir():
            raise ProbeError(f"no such directory of probes: {directory}")
        for path in _probes_in(directory):
            name = path.stem[len(PREFIX) :]
            probes.setdefault(name, Probe(name, path, directory, summary_of(path)))
    for path in _probes_in(SHIPPED_DIR):
        name = path.stem[len(PREFIX) :]
        if name in probes:
            own = probes[name]
            probes[name] = Probe(
                own.name, own.path, own.origin, own.summary, hides=path
            )
        else:
            probes[name] = Probe(name, path, SHIPPED_DIR, summary_of(path))
    return probes


def find_probe(probes, spelling):
    """A probe by name, written with or without ``probe_`` and ``.py``."""
    name = spelling
    if name.endswith(".py"):
        name = name[: -len(".py")]
    if name.startswith(PREFIX):
        name = name[len(PREFIX) :]
    if name not in probes:
        known = ", ".join(sorted(probes)) or "none"
        raise ProbeError(f"no probe named {spelling!r}; available: {known}")
    return probes[name]


def split_arguments(words):
    """The probe's own arguments and the documents, split at the first word that is an existing path.

    Arguments come first and documents last, which is the one convention every probe follows; it lets
    the split be made without knowing anything about the probe.
    """
    arguments, documents = [], []
    for word in words:
        path = Path(word)
        if path.exists():
            documents.append(path)
        elif documents:
            raise ProbeError(
                f"{word!r} comes after the documents: a probe's own arguments go before them"
            )
        else:
            arguments.append(word)
    return arguments, documents


def command_for(probe, arguments, documents):
    """The process to start: the probe, run by the interpreter the tool itself runs under."""
    return [sys.executable, str(probe.path), *arguments, *(str(d) for d in documents)]


def run(probe, arguments, documents):
    """Run a probe, letting its output through; returns its exit status."""
    return subprocess.run(
        command_for(probe, arguments, documents), check=False
    ).returncode


def format_listing(probes):
    """One line per probe: the name, the question, and where it comes from if it is not shipped."""
    if not probes:
        return "no probes found"
    width = max(len(name) for name in probes)
    lines = []
    for name in sorted(probes):
        probe = probes[name]
        line = f"{name:<{width}}  {probe.summary}"
        if probe.origin != SHIPPED_DIR:
            line += f"  [{probe.path}"
            line += ", hides the shipped probe]" if probe.hides else "]"
        lines.append(line)
    return "\n".join(lines)
