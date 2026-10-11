"""The document-classification test of a formats repository: is each test report recognised as
the format whose folder it sits in?

Phase 13 of ``agent-memory/PLAN-document-classification.md``. One test per document
(:class:`freeports_dev.pytest_plugin.ReportVariant`), named ``test_classification``. It passes when
the engine classifies the document as the folder's format, or does not classify it at all, or
stops at a partial that the folder's format refines; it fails when the document is classified as
another format, when a partial excludes the folder's format, and when the classifier errs.

The engine is asked through ``freeports classify`` (one subprocess per document) and its default
one-line-per-document output is parsed: there is no Python entry point for document
classification, and adding one is not this package's call.
"""

import subprocess
from dataclasses import dataclass, field

#: The label given to the one document passed to ``freeports classify`` (``-i <path>:report``), so
#: that the output line to read is ``report: <outcome>`` whatever the path looks like.
DOCUMENT_LABEL = "report"

COMPLETE = "complete"
PARTIAL = "partial"
NOT_CLASSIFIED = "not_classified"
ERROR = "error"


class ClassifyOutputError(Exception):
    """``freeports classify`` printed nothing this module can read for the document."""


@dataclass(frozen=True)
class ClassifyOutcome:
    """What ``freeports classify`` said about one document."""

    kind: str
    format: str = None
    candidates: tuple = field(default_factory=tuple)
    by_exclusion: bool = False
    message: str = None

    @classmethod
    def complete(cls, format_name, by_exclusion=False):
        return cls(kind=COMPLETE, format=format_name, by_exclusion=by_exclusion)

    @classmethod
    def partial(cls, pattern, candidates):
        return cls(kind=PARTIAL, format=pattern, candidates=tuple(candidates))

    @classmethod
    def not_classified(cls):
        return cls(kind=NOT_CLASSIFIED)

    @classmethod
    def error(cls, message):
        return cls(kind=ERROR, message=message)


def parse_classify_output(stdout, label=DOCUMENT_LABEL):
    """The outcome on the ``<label>: …`` line of ``freeports classify``'s default output.

    Raises :class:`ClassifyOutputError` when there is no such line or it is not one of the four
    forms (complete, possibly ``(by exclusion)``; ``partial P — candidates: …``; ``not
    classified``; ``error: …``).
    """
    prefix = f"{label}: "
    for line in stdout.splitlines():
        if line.startswith(prefix):
            return _parse_outcome(line[len(prefix) :], line)
    raise ClassifyOutputError(
        f"no line for {label!r} in the output of freeports classify"
    )


def _parse_outcome(text, line):
    if text == "not classified":
        return ClassifyOutcome.not_classified()
    if text.startswith("error: "):
        return ClassifyOutcome.error(text[len("error: ") :])
    if text.partition(" ")[0] == "partial":
        pattern, separator, candidates = text.partition(" ")[2].partition(
            " — candidates: "
        )
        if not separator:
            raise ClassifyOutputError(f"unreadable partial outcome: {line!r}")
        return ClassifyOutcome.partial(pattern, candidates.split(", "))
    format_name, by_exclusion = _strip_suffix(text, " (by exclusion)")
    if not format_name or " " in format_name:
        raise ClassifyOutputError(f"unreadable outcome: {line!r}")
    return ClassifyOutcome.complete(format_name, by_exclusion=by_exclusion)


def _strip_suffix(text, suffix):
    if text.endswith(suffix):
        return text[: -len(suffix)], True
    return text, False


def classification_failure(folder_format, outcome):
    """``None`` when ``outcome`` is acceptable for a document under ``folder_format``, otherwise
    the message the failing test reports."""
    if outcome.kind == NOT_CLASSIFIED:
        return None
    if outcome.kind == ERROR:
        return f"the classification of a {folder_format} document failed: {outcome.message}"
    if outcome.kind == COMPLETE:
        if outcome.format == folder_format:
            return None
        return f"a {folder_format} document is classified as {outcome.format}"
    if folder_format in outcome.candidates:
        return None
    return (
        f"a {folder_format} document is classified as partial {outcome.format}, "
        f"which {folder_format} does not refine (candidates: {', '.join(outcome.candidates)})"
    )


def classify_document(pdf_path, repo):
    """Run ``freeports classify`` on one document against ``repo`` and return its outcome.

    Raises :class:`ClassifyOutputError` (carrying the command's stderr) when the command fails
    without printing an outcome for the document — a classifier that does not load, for one.
    """
    completed = subprocess.run(
        [
            "freeports",
            "classify",
            "-qqq",
            "-F",
            str(repo),
            "-i",
            f"{pdf_path}:{DOCUMENT_LABEL}",
        ],
        capture_output=True,
        text=True,
    )
    try:
        return parse_classify_output(completed.stdout)
    except ClassifyOutputError as exc:
        raise ClassifyOutputError(
            f"freeports classify exited with {completed.returncode} and no outcome for "
            f"{pdf_path}: {completed.stderr.strip()}"
        ) from exc
