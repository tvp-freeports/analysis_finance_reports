"""Turning each tool's own JSON into the one record shape the gate reads.

Every measurement tool here answers in its own dialect: ``cargo llvm-cov`` nests totals under
``data[0].totals.lines.percent``, ``coverage.py`` puts them at ``totals.percent_covered``, rustdoc
gives a map of files to ``{total, with_docs}`` with an ``#ALL#`` row that is a total and not a file.
Knowledge of those dialects lives here and nowhere else.

The alternative was a shell fragment per tool inside a Makefile recipe, and it is worth saying why
that was rejected: those fragments cannot be tested. A reader in this module is exercised by a
fixture that is a captured output of the real tool, so the day ``llvm-cov`` moves a key the suite
says so — instead of a Makefile quietly producing an empty string, the gate reading it as zero, and
a commit being refused for a reason nobody can find.

Each reader takes parsed JSON and returns a :class:`~freeports_dev.ci.report.Measurement`. None of
them runs anything: what produced the file is the caller's business, which is what makes them
testable without the tool installed.
"""

import json
import re
from pathlib import Path

from freeports_dev.ci import report


class ReaderError(Exception):
    """The file is not the report this reader was told it was.

    Distinct from an unmeasured metric, and deliberately: unmeasured means nobody could compute the
    figure, this means somebody computed something and it was handed to the wrong reader. Telling
    them apart is the difference between "install cargo-llvm-cov" and "you passed --from the wrong
    tool", which are different things to say to a person.
    """


def _load(payload):
    if isinstance(payload, (str, bytes)):
        try:
            return json.loads(payload)
        except ValueError as exc:
            raise ReaderError(f"not JSON: {exc}") from exc
    if isinstance(payload, Path):
        return _load(payload.read_text(encoding="utf-8"))
    return payload


def from_llvm_cov(payload, metric="tests.rust.lines", head=None):
    """``cargo llvm-cov --json --summary-only`` — line coverage of the crate.

    The line figure and not the region or function one: lines are what the other coverage metric in
    this repository counts, and two numbers called "coverage" that count different things would be
    two numbers nobody can compare.
    """
    doc = _load(payload)
    try:
        totals = doc["data"][0]["totals"]
        lines = totals["lines"]
        value = float(lines["percent"])
    except (KeyError, IndexError, TypeError, ValueError) as exc:
        raise ReaderError(f"not an llvm-cov summary: {exc}") from exc

    breakdown = {}
    for entry in doc["data"][0].get("files", []) or []:
        name = entry.get("filename")
        summary = (entry.get("summary") or {}).get("lines") or {}
        if name and "percent" in summary:
            breakdown[name] = float(summary["percent"])

    return report.Measurement(
        metric,
        value=value,
        unit="percent",
        head=head,
        breakdown=breakdown,
        detail={"covered": lines.get("covered"), "total": lines.get("count")},
    )


def from_coverage_py(payload, metric="tests.python.lines", head=None):
    """``pytest --cov --cov-report=json`` — line coverage of one Python package, or of several.

    ``payload`` may be a list of reports, and then the figure is recomputed over their combined
    counts rather than averaged over the packages. Averaging would give a fifty-line package the
    same weight as a five-thousand-line one, which is how an aggregate ends up saying something
    nobody meant. The two packages here differ by an order of magnitude in size, so this is not a
    hypothetical difference.
    """
    documents = payload if isinstance(payload, list) else [_load(payload)]
    documents = [_load(doc) for doc in documents]

    covered = 0
    statements = 0
    breakdown = {}
    for doc in documents:
        try:
            totals = doc["totals"]
            covered += int(totals["covered_lines"])
            statements += int(totals["num_statements"])
        except (KeyError, TypeError, ValueError) as exc:
            raise ReaderError(f"not a coverage.py report: {exc}") from exc
        for name, entry in (doc.get("files") or {}).items():
            if isinstance(entry, dict) and "summary" in entry:
                breakdown[name] = float(entry["summary"]["percent_covered"])

    return report.Measurement(
        metric,
        value=100.0 * covered / statements if statements else 100.0,
        unit="percent",
        head=head,
        breakdown=breakdown,
        detail={"covered": covered, "total": statements},
    )


#: The row rustdoc uses for the crate total. It is not a file and must not be counted as one.
_RUSTDOC_TOTAL = "#ALL#"


def from_rustdoc(payload, metric="docs.rust", head=None):
    """``cargo +nightly rustdoc -- --show-coverage --output-format json``.

    The percentage is recomputed from the item counts rather than read off the ``#ALL#`` row.
    Recomputing costs nothing and makes the figure and its ``covered/total`` detail arithmetically
    consistent, so a person checking the report by hand gets the number the report printed.
    """
    doc = _load(payload)
    if not isinstance(doc, dict):
        raise ReaderError("not a rustdoc coverage report")

    documented = 0
    total = 0
    breakdown = {}
    for name, entry in doc.items():
        if not isinstance(entry, dict) or "total" not in entry:
            continue
        if name == _RUSTDOC_TOTAL:
            continue
        with_docs = int(entry.get("with_docs") or 0)
        items = int(entry.get("total") or 0)
        documented += with_docs
        total += items
        if items:
            breakdown[name] = 100.0 * with_docs / items

    if not total:
        raise ReaderError("a rustdoc coverage report with no items in it")

    return report.Measurement(
        metric,
        value=100.0 * documented / total,
        unit="percent",
        head=head,
        breakdown=breakdown,
        detail={"documented": documented, "total": total},
    )


def from_ruff(payload, metric="lint.python", head=None, statements=None, files=None):
    """Ruff's JSON, scored.

    The denominator belongs to the caller, which knows which files it handed the linter; passing
    ``statements`` skips the recount. Left out, the files are read and their lines counted, which is
    right when the report is all that survived of the run.
    """
    from freeports_dev.ci import lint

    result = lint.parse_ruff(_load(payload), files)
    if statements is not None:
        result.statements = statements
    return result.measurement(metric, head)


def from_clippy(payload, metric="lint.rust", head=None, statements=0, files=None):
    """Cargo's JSON message stream, scored.

    ``statements`` is the caller's for the same reason: cargo's messages name the files that had a
    finding, which is not the file set clippy was given, and using it as the denominator would make
    the score *rise* as more warnings appeared in more files.
    """
    from freeports_dev.ci import lint

    lines = payload.splitlines() if isinstance(payload, str) else payload
    return lint.parse_clippy(lines, statements, files).measurement(metric, head)


def from_validate(payload, metric="grants.coverage", head=None):
    """``freeports-validate collect`` — the grant model, read for its coverage ratio.

    The model rather than a second walk of ``validation/``: ``collect`` resolves each methodology
    page once, and the engine's ``make validation-report`` already collects once and renders many
    times. Reading its output makes this one more rendering out of that same walk, so a commit
    still fetches each page once rather than once per thing that wants a number from it.

    A model whose ``coverage`` has no denominator is *unmeasured*, not 100 %. A repository where
    nothing could be resolved has not proved its grants are complete; it has proved nothing, and
    saying otherwise is the failure this project already refuses for an unreachable methodology
    page — unverifiable is not granted.
    """
    doc = _load(payload)
    if not isinstance(doc, dict) or "coverage" not in doc:
        raise ReaderError("not a freeports-validate model")
    coverage = doc.get("coverage") or {}
    ratio = coverage.get("ratio")
    candidates = coverage.get("candidates")
    if ratio is None or not candidates:
        return report.Measurement.unmeasured(
            metric,
            "freeports-validate found no file a grant could cover, so there is no ratio to "
            "report. Run `freeports-validate collect` and check its output.",
            "percent",
            head,
        )
    return report.Measurement(
        metric,
        value=100.0 * float(ratio),
        unit="percent",
        head=head,
        breakdown={
            entry.get("name", str(index)): entry.get("coverage", {}).get("ratio")
            for index, entry in enumerate(doc.get("methodologies") or [])
            if isinstance(entry, dict)
        },
        detail={"granted": coverage.get("granted"), "candidates": candidates},
    )


def from_check_keys(payload, metric="grants.keys_online", head=None):
    """``freeports-validate check-keys``'s summary line, read for the published fraction.

    A key that could not be checked makes the metric **unmeasured** rather than lowering it. That is
    the same distinction the subcommand itself draws with its third exit status, and the same one
    ``check-grants`` draws between a broken grant and an unverifiable one: reporting "I could not
    reach the server" as a number would turn a fact about the network into a fact about the
    repository.
    """
    text = payload.decode() if isinstance(payload, bytes) else str(payload)
    found = re.search(
        r"published\s+(\d+)/(\d+),\s*not published\s+(\d+),\s*could not be checked\s+(\d+)",
        text,
    )
    if not found:
        raise ReaderError("not a check-keys summary")
    published, total, _missing, unknown = (int(g) for g in found.groups())
    if unknown:
        return report.Measurement.unmeasured(
            metric,
            f"{unknown} of {total} keys could not be looked up, so the fraction published is not "
            f"known. Nothing here says those keys are missing — it says nobody looked.",
            "percent",
            head,
        )
    if not total:
        return report.Measurement(
            metric, 100.0, "percent", head=head, detail={"published": 0, "total": 0}
        )
    return report.Measurement(
        metric,
        value=100.0 * published / total,
        unit="percent",
        head=head,
        detail={"published": published, "total": total},
    )


#: Every reader, by the name ``--from`` takes.
READERS = {
    "llvm-cov": from_llvm_cov,
    "coverage-py": from_coverage_py,
    "rustdoc": from_rustdoc,
    "ruff": from_ruff,
    "clippy": from_clippy,
    "validate": from_validate,
    "check-keys": from_check_keys,
}
