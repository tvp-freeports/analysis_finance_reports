"""``lint.rust`` and ``lint.python``: a linter's findings as a score out of ten.

::

    score = max(0, 10 - 10 * (5 * errors + warnings) / statements)

**Pylint's own formula**, for two reasons. The number then means what it meant on the trend graph
this project used to plot, so a figure from before this work and one from after are the same
quantity. And a raw count is not comparable between a 2 900-line repository and a 44 000-line one:
eight warnings is close to spotless in the crate and would be a bad afternoon in a formats
repository's ``content/``. A threshold has to survive the codebase growing, and a count does not.

An error weighs five warnings because that is pylint's weighting and there is no reason here to
invent a different one.

``statements`` is approximated by **non-blank, non-comment source lines over the same files the
linter was given**. It is an approximation and is documented as one: pylint counts statements from
its own AST, and a line is not a statement. What it has to be is *stable* — the same denominator
today and next month for the same code — and it is, which is all a ratchet needs.

**The raw counts print beside the score, always.** A score is what a threshold compares; a count is
what a person fixes. Showing only the score would be showing only the half nobody can act on.
"""

import json
import re
import subprocess
from pathlib import Path

from freeports_dev.ci import report


#: What an error costs, in warnings. Pylint's weighting, kept so the number stays comparable.
ERROR_WEIGHT = 5


def score(errors, warnings, statements):
    """Pylint's score, floored at zero and reading ten for a codebase with nothing in it.

    Ten for an empty file set rather than a division by zero: a repository with no linted source
    has no violation in it, and refusing a commit over that would be a gate teaching people to
    switch it off.
    """
    if not statements:
        return 10.0
    return max(0.0, 10.0 - 10.0 * (ERROR_WEIGHT * errors + warnings) / statements)


_PYTHON_COMMENT = re.compile(r"^\s*#")


def count_statements(paths):
    """Non-blank, non-comment lines over a set of files — the denominator, approximated.

    Only whole-line comments are dropped: a trailing comment sits on a line that also carries code,
    and that line is a statement. Docstrings and block comments are *not* excluded, deliberately —
    excluding them would make the score rise as documentation is deleted, and a metric that rewards
    deleting documentation next to one that measures documentation would be an embarrassment.
    """
    total = 0
    for path in paths:
        try:
            text = Path(path).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for line in text.splitlines():
            stripped = line.strip()
            if not stripped or _PYTHON_COMMENT.match(line) or stripped.startswith("//"):
                continue
            total += 1
    return total


class LintResult:
    """One linter's findings, already split the way the formula wants them."""

    def __init__(self, errors=0, warnings=0, statements=0, files=None, findings=None):
        self.errors = errors
        self.warnings = warnings
        self.statements = statements
        self.files = list(files or [])
        self.findings = list(findings or [])

    @property
    def score(self):
        return score(self.errors, self.warnings, self.statements)

    @property
    def total(self):
        return self.errors + self.warnings

    def by_rule(self):
        """How many findings each rule accounts for — what somebody reads to decide where to start."""
        counts = {}
        for rule in self.findings:
            counts[rule] = counts.get(rule, 0) + 1
        return dict(sorted(counts.items(), key=lambda item: (-item[1], item[0])))

    def measurement(self, metric, head=None):
        return report.Measurement(
            metric,
            value=self.score,
            unit="score",
            head=head,
            breakdown=self.by_rule(),
            detail={
                "errors": self.errors,
                "warnings": self.warnings,
                "statements": self.statements,
                "files": len(self.files),
            },
        )


# -- ruff ------------------------------------------------------------------------------------


def parse_ruff(payload, paths=None):
    """Ruff's JSON output, counted.

    Every ruff violation weighs one. Ruff has no error/warning split of its own — a rule is either
    enabled or it is not — and inventing a severity here would be this tool deciding which of the
    repository's own enabled rules it takes seriously, which is the repository's decision and is
    already made in its ``pyproject.toml``.
    """
    diagnostics = json.loads(payload) if isinstance(payload, (str, bytes)) else payload
    findings = []
    files = set()
    for item in diagnostics or []:
        findings.append(item.get("code") or "unknown")
        if item.get("filename"):
            files.add(item["filename"])
    file_set = list(paths) if paths is not None else sorted(files)
    return LintResult(
        errors=0,
        warnings=len(findings),
        statements=count_statements(file_set),
        files=file_set,
        findings=findings,
    )


def run_ruff(root, targets=None):
    """Run ruff over a repository and count what it says.

    Missing ruff is *unmeasured*, never a pass: the answer to "is this code clean" cannot be yes
    because nobody looked. The reason names the tool so the message a refused commit prints is
    something a person can act on.
    """
    root = Path(root)
    targets = [str(t) for t in (targets or [root])]
    try:
        done = subprocess.run(
            ["ruff", "check", "--output-format", "json", *targets],
            cwd=str(root),
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError:
        return None, "ruff is not installed. Install it with `pip install ruff`."
    if done.returncode not in (0, 1):
        return None, f"ruff failed: {done.stderr.strip() or done.stdout.strip()}"
    try:
        diagnostics = json.loads(done.stdout or "[]")
    except ValueError:
        return None, "ruff produced no JSON report."
    scanned = [
        str(path)
        for target in targets
        for path in sorted(Path(root, target).rglob("*.py"))
        if "__pycache__" not in path.parts
    ] or None
    return parse_ruff(diagnostics, scanned), None


# -- clippy ----------------------------------------------------------------------------------


def parse_clippy(lines, statements=0, files=None):
    """Cargo's JSON message stream, counted.

    Only ``compiler-message`` records with a level of ``warning`` or ``error`` count, and each is
    counted once. Cargo also emits a summary diagnostic — "generated 8 warnings" — which carries no
    code and would double every figure if it were counted, so a message with no ``code`` and no
    primary span is dropped.
    """
    errors = 0
    warnings = 0
    findings = []
    for line in (
        lines if not isinstance(lines, (str, bytes)) else str(lines).splitlines()
    ):
        line = line.strip() if isinstance(line, str) else line
        if not line:
            continue
        try:
            record = json.loads(line) if isinstance(line, str) else line
        except ValueError:
            continue
        if record.get("reason") != "compiler-message":
            continue
        message = record.get("message") or {}
        level = message.get("level")
        if level not in ("warning", "error"):
            continue
        code = (message.get("code") or {}).get("code")
        spans = message.get("spans") or []
        if not code and not any(span.get("is_primary") for span in spans):
            continue
        findings.append(code or level)
        if level == "error":
            errors += 1
        else:
            warnings += 1
    return LintResult(
        errors=errors,
        warnings=warnings,
        statements=statements,
        files=files or [],
        findings=findings,
    )


def rust_files(crate_root):
    """The crate's own sources — the file set clippy was given, and so the denominator's."""
    src = Path(crate_root) / "src"
    if not src.is_dir():
        return []
    return sorted(p for p in src.rglob("*.rs") if "target" not in p.parts)


def run_clippy(crate_root):
    """Run clippy over a crate and count what it says."""
    crate_root = Path(crate_root)
    try:
        done = subprocess.run(
            [
                "cargo",
                "clippy",
                "--all-targets",
                "--message-format",
                "json",
                "--manifest-path",
                str(crate_root / "Cargo.toml"),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError:
        return None, "cargo is not installed, so clippy could not be run."
    if not done.stdout.strip():
        return None, f"clippy produced no report: {done.stderr.strip()[:400]}"
    files = rust_files(crate_root)
    return parse_clippy(done.stdout.splitlines(), count_statements(files), files), None
