"""``freeports-dev coverage`` for a formats repository: how much of it is actually tested.

Two figures, both over the same unit — the **document**, which is a format holding one
``report.pdf`` or one subdirectory of a format holding several:

* ``tests.formats.integration`` — documents whose whole-document test exists at all, which is to
  say documents with an ``out/`` for the run to be checked against;
* ``tests.formats.single_page`` — documents carrying at least one page with the whole fixture
  triple, so that every stage of the pipeline is pinned somewhere in that document.

They answer different questions and a repository can be strong in one and weak in the other, which
is exactly why they are two thresholds and not an average. A repository at 100 % single-page and
86 % integration — which is this workspace's formats repository today — has every format's stages
pinned page by page and five documents nobody has ever run end to end. One number would hide that.

The walk is :mod:`freeports_dev.format_inventory`, shared with the pytest plugin so that the report
cannot claim a coverage the suite does not collect. It reads file names and nothing else: no PDF is
opened, no algorithm is loaded, so this is a **fast** metric and belongs in the commit hook.
"""

from freeports_dev.ci import report
from freeports_dev.format_inventory import scan_repository


#: The two metrics this command answers for.
INTEGRATION = "tests.formats.integration"
SINGLE_PAGE = "tests.formats.single_page"


class FormatsCoverage:
    """The inventory of a formats repository, and the two ratios read off it."""

    def __init__(self, documents):
        self.documents = list(documents)

    @property
    def total(self):
        return len(self.documents)

    @property
    def with_integration(self):
        return [d for d in self.documents if d.has_integration]

    @property
    def with_single_page(self):
        return [d for d in self.documents if d.has_single_page_triple]

    @property
    def missing_integration(self):
        """The documents holding the integration figure down, by name, in order.

        Named rather than counted because a percentage is what a threshold compares and a list of
        names is what somebody acts on. Every rendering prints both.
        """
        return [d.name for d in self.documents if not d.has_integration]

    @property
    def missing_single_page(self):
        return [d.name for d in self.documents if not d.has_single_page_triple]

    @property
    def problems(self):
        """Malformed documents, as ``(name, message)``, from the same walk.

        A repository with one broken document is still measured and the breakage is named. A report
        that refuses to print because one document is malformed tells you less than one that prints
        and says which.
        """
        return [(d.name, message) for d in self.documents for message in d.problems]

    def ratio(self, covered):
        """A percentage, with an empty repository reading 100 rather than dividing by zero.

        An empty repository has no untested document in it. Calling that 0 % would refuse the first
        commit to a repository that has nothing wrong with it, which is how a gate teaches people
        to turn it off.
        """
        return 100.0 if not self.total else 100.0 * len(covered) / self.total

    def measurements(self, head=None):
        """Both metrics, in the normalised shape the gate reads."""
        return [
            report.Measurement(
                INTEGRATION,
                value=self.ratio(self.with_integration),
                unit="percent",
                head=head,
                breakdown={d.name: d.has_integration for d in self.documents},
                detail={
                    "covered": len(self.with_integration),
                    "total": self.total,
                    "missing": self.missing_integration,
                },
            ),
            report.Measurement(
                SINGLE_PAGE,
                value=self.ratio(self.with_single_page),
                unit="percent",
                head=head,
                breakdown={d.name: sorted(d.triple_pages) for d in self.documents},
                detail={
                    "covered": len(self.with_single_page),
                    "total": self.total,
                    "missing": self.missing_single_page,
                },
            ),
        ]


def measure(repo):
    """Walk a formats repository and read both figures off it."""
    return FormatsCoverage(scan_repository(repo))


# -- renderings ------------------------------------------------------------------------------
#
# The same four `freeports-validate report` offers, and for the same reason: these figures belong
# in a repository's README next to the grant badges, and a format author should not have to
# transcribe them by hand from a terminal.


def render_text(coverage):
    lines = []
    total = coverage.total
    lines.append(f"{total} document{'' if total == 1 else 's'} in this repository")
    lines.append("")
    for label, metric, covered, missing in (
        (
            "integration",
            INTEGRATION,
            coverage.with_integration,
            coverage.missing_integration,
        ),
        (
            "single page",
            SINGLE_PAGE,
            coverage.with_single_page,
            coverage.missing_single_page,
        ),
    ):
        ratio = coverage.ratio(covered)
        lines.append(
            f"{label:<12} {ratio:6.1f} %   {len(covered)}/{total}   ({metric})"
        )
        if missing:
            lines.append(f"{'':<12} missing: {', '.join(missing)}")
    if coverage.problems:
        lines.append("")
        lines.append("documents the rules could not describe:")
        for name, message in coverage.problems:
            lines.append(f"  {name}: {message}")
    return "\n".join(lines)


def render_markdown(coverage):
    total = coverage.total
    lines = [
        "| Metric | Coverage | Documents |",
        "|---|---|---|",
    ]
    for label, covered in (
        ("Integration", coverage.with_integration),
        ("Single page", coverage.with_single_page),
    ):
        lines.append(
            f"| {label} | {coverage.ratio(covered):.1f} % | {len(covered)}/{total} |"
        )
    if coverage.missing_integration:
        lines.append("")
        lines.append(
            "No whole-document test: "
            + ", ".join(f"`{n}`" for n in coverage.missing_integration)
        )
    if coverage.missing_single_page:
        lines.append("")
        lines.append(
            "No page carrying the whole fixture triple: "
            + ", ".join(f"`{n}`" for n in coverage.missing_single_page)
        )
    return "\n".join(lines)


def render_badges(coverage):
    """Two shields.io endpoints, coloured on the same scale the grant badges already use."""
    lines = []
    for label, covered in (
        ("integration", coverage.with_integration),
        ("single%20page", coverage.with_single_page),
    ):
        ratio = coverage.ratio(covered)
        colour = "brightgreen" if ratio >= 90 else "yellow" if ratio >= 70 else "red"
        lines.append(
            f"![{label}](https://img.shields.io/badge/{label}-{ratio:.0f}%25-{colour})"
        )
    return "\n".join(lines)


def render_json(coverage, head=None):
    import json

    return json.dumps(
        {m.metric: m.to_dict() for m in coverage.measurements(head)},
        indent=2,
        sort_keys=True,
    )
