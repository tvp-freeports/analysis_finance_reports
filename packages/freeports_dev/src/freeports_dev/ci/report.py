"""The shape every measurement writes, and how one is read back.

A measurement command answers one question and writes one small file into ``reports/``. It decides
nothing: the gate reads these files, applies ``ci.yaml``, and chooses the exit status. Keeping the
two apart is what makes a threshold survivable — when a commit is refused, the number that refused
it is sitting in a file a person can open, produced by a command they can re-run by hand.

::

    {"metric": "docs.rust", "value": 39.4, "unit": "percent", "state": "measured",
     "reason": null, "head": "3f2a...", "breakdown": {"src/cli/batch.rs": 8.7},
     "detail": {"covered": 377, "total": 957}}

``state`` is the field that matters. **A figure nobody could compute is not a figure above the
threshold**, so a measurement that failed says ``unmeasured`` and carries the sentence a person
reads instead of quietly reporting zero — which would be a lie in one direction — or being absent,
which the gate would have to guess about. It is the same distinction this project already draws for
a methodology page it could not reach: unverifiable is not granted.

``head`` is the commit the figure was taken at, and it is what makes staleness visible. The commit
hook measures the fast metrics now and *reads* the slow ones from the last full run; without the
commit recorded beside the number there would be no way to tell a fresh figure from one taken three
weeks ago, and the fast/slow split would quietly become a way of never measuring the slow half.
"""

import json
import subprocess
from pathlib import Path


#: The figure was computed and can be compared against a minimum.
MEASURED = "measured"

#: The figure could not be computed. Never a pass, on any branch.
UNMEASURED = "unmeasured"

#: Where a repository's measurements are written. Gitignored in every repository kind.
REPORTS_DIR = "reports"


class Measurement:
    """One metric's answer, with everything the gate needs to judge it and a person needs to fix it."""

    def __init__(
        self,
        metric,
        value=None,
        unit=None,
        state=MEASURED,
        reason=None,
        head=None,
        breakdown=None,
        detail=None,
    ):
        self.metric = metric
        self.value = value
        self.unit = unit
        self.state = state
        self.reason = reason
        self.head = head
        self.breakdown = breakdown or {}
        self.detail = detail or {}

    @classmethod
    def unmeasured(cls, metric, reason, unit=None, head=None):
        """A metric whose tool is missing or whose run failed, and the sentence that says which.

        The reason is not optional and is not a category: it is what a person reads at the moment a
        commit is refused, so it names the tool and, where there is one, the command that installs
        it.
        """
        return cls(metric, None, unit, UNMEASURED, reason, head)

    @property
    def is_measured(self):
        return self.state == MEASURED and self.value is not None

    def to_dict(self):
        return {
            "metric": self.metric,
            "value": self.value,
            "unit": self.unit,
            "state": self.state,
            "reason": self.reason,
            "head": self.head,
            "breakdown": self.breakdown,
            "detail": self.detail,
        }

    @classmethod
    def from_dict(cls, doc):
        return cls(
            metric=doc.get("metric"),
            value=doc.get("value"),
            unit=doc.get("unit"),
            state=doc.get("state", MEASURED),
            reason=doc.get("reason"),
            head=doc.get("head"),
            breakdown=doc.get("breakdown"),
            detail=doc.get("detail"),
        )

    def write(self, path):
        path = Path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            json.dumps(self.to_dict(), indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        return path


def file_name(metric):
    """The file a metric is written to: the dotted name with dots as dashes, plus ``.json``.

    Mechanical, so that a person who knows the metric can find the file without a lookup table, and
    so that the gate can read a directory of them without a manifest saying what is in it.
    """
    return metric.replace(".", "-") + ".json"


def read_all(reports_dir):
    """Every measurement in ``reports/``, by metric name.

    A file that does not parse is skipped rather than fatal: the directory is a cache of separate
    runs, one of which may have been interrupted, and one bad file must not stop the gate from
    judging the other eleven metrics. The metric it held then reads as having no measurement at
    all, which the gate already knows how to report.
    """
    directory = Path(reports_dir)
    found = {}
    if not directory.is_dir():
        return found
    for path in sorted(directory.glob("*.json")):
        try:
            doc = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        if isinstance(doc, dict) and doc.get("metric"):
            found[doc["metric"]] = Measurement.from_dict(doc)
    return found


def head_commit(root):
    """The commit a measurement is being taken at, or ``None`` where there is no git or no commit.

    ``None`` is honest and the gate treats it as "staleness cannot be judged here" rather than as
    stale — a repository with no commits has nothing for a figure to be stale against.
    """
    try:
        done = subprocess.run(
            ["git", "-C", str(root), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError:
        return None
    return done.stdout.strip() if done.returncode == 0 and done.stdout.strip() else None
