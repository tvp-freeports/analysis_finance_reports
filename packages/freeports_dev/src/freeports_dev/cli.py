"""The `freeports-dev` command line.

Every subcommand takes its settings from a :class:`~freeports_dev.config.DevConfig` rather than
reading the environment itself, so that one setting resolves the same way whichever subcommand asked
for it. The options shared with the engine — the repository, the input database, the configuration
file, the target lists — are declared once, on the parent parsers below, and carry the same names and
short letters `freeports` uses.
"""

import argparse
import sys
from pathlib import Path

from freeports_dev.config import DevConfig, set_active


def _repo_or_exit(config):
    """The formats repository, checked to actually be one.

    The same guard three subcommands had inline, in one place: pointing any of them at a directory
    that is not a formats repository fails the same way, with the same message.
    """
    repo = config.formats_repo
    if not (repo / "metadata" / "formats.csv").exists():
        print(
            f"Error: {repo} does not appear to be a formats repository "
            f"(missing metadata/formats.csv)"
        )
        sys.exit(1)
    return repo


#: The two halves of a formats repository's suite: the marker that selects each, the suite it is
#: recorded under, and what a run of it does *not* cover.
#:
#: One entry per half, named after the Makefile target that runs it — ``--fast`` is ``make
#: test-fast`` and ``--slow`` is ``make test-slow``. The two surfaces are deliberately one
#: vocabulary: a person who has learnt the engine's Makefile can work in a formats repository
#: without reading anything, and a person who has learnt this command can run the suite of a
#: repository they have just cloned. Two names for the same selection is how they come apart.
TEST_SELECTIONS = {
    "fast": (
        "not integration_tests",
        "formats.single_page",
        "the per-page tests: one page of one document at a time",
        "The whole-document tests did not run here: `freeports-dev test --slow`, or --all for both.",
    ),
    "slow": (
        "integration_tests",
        "formats.integration",
        "the whole-document tests: a full extraction run per document",
        "The per-page tests did not run here: `freeports-dev test --fast`, or --all for both.",
    ),
    "all": (None, None, "every test in this repository", None),
}

#: pytest's exit status for "nothing was collected", which is **not** a failure here.
#:
#: A repository `init-format-repo` has just created has no formats in it yet, and a suite with no
#: tests in it has not failed -- it has passed vacuously, the way an empty directory of tests
#: passes. Reading 5 as a failure made every freshly bootstrapped repository report a broken suite
#: until somebody wrote a format, which is exactly the moment a new user decides whether to trust
#: any of this. The same reading is why `--slow` on a repository whose formats have no
#: whole-document test yet is not a red suite.
NOTHING_COLLECTED = 5


def _cmd_test(args):
    import pytest

    config = DevConfig(args)
    repo = _repo_or_exit(config)
    set_active(config)

    test_dir = (
        repo / "tests" / "formats" / args.format if args.format else repo / "tests"
    )
    pytest_args = [str(test_dir), "--rootdir", str(repo)]
    extra = args.pytest_args
    if extra and extra[0] == "--":
        extra = extra[1:]

    marker, _suite, covers, left_out = TEST_SELECTIONS[args.select]

    # A `-m` of your own outranks the flag, and is left to stand alone.
    #
    # pytest takes the *last* `-m` on the command line, so adding one beside `-- -m 'not
    # integration_tests and not xfail'` would not even be an error: it would quietly replace the
    # selection that was asked for with the one this flag defaults to. Selections this command has
    # no vocabulary for have to stay possible, and this is what keeps them so.
    own_marker = any(
        argument == "-m" or argument.startswith("-m") for argument in extra
    )
    if marker and not own_marker:
        pytest_args += ["-m", marker]

    if not own_marker:
        print(f"Running {covers}.")

    pytest_args.extend(extra)
    outcome = pytest.main(pytest_args)

    if outcome == NOTHING_COLLECTED:
        print(
            "\nNo test matched this selection, which is not a failure: there is nothing here to"
        )
        print(
            "fail. `freeports-dev coverage` says which documents have tests and which have not."
        )
        outcome = 0

    # Said after the run rather than before it, where a reader looks once the summary has printed.
    # A gate that runs half the tests and says nothing about the other half is a gate that lies;
    # this is the same rule the recorded suite outcomes follow, in the one place a person is
    # looking at the result rather than at the report.
    if left_out and not own_marker:
        print(left_out)

    sys.exit(outcome)


def _cmd_make_tests(args):
    config = DevConfig(args)
    repo = _repo_or_exit(config)

    from freeports_dev.make_tests import add_page_test

    base_path = repo / "tests" / "formats"
    filter_data = None
    if args.filter_data:
        import dill

        with open(args.filter_data, "rb") as f:
            filter_data = dill.load(f)

    add_page_test(
        fmt=args.format,
        document=args.document,
        page_type=config.page_type,
        n_page=args.page,
        base_out_path=base_path,
        base_in_path=base_path,
        report_file=args.report,
        filter_data=filter_data,
        noconfirm=config.noconfirm,
        skip_pdf_blks=args.skip_pdf_blks,
        skip_txt_blks=args.skip_txt_blks,
        skip_results=args.skip_results,
        print_results=not args.noprint_results,
        print_txt_blks=args.print_txt_blks,
        print_pdf_blks=args.print_pdf_blks,
    )


def _cmd_inspect_page(args):
    config = DevConfig(args)
    repo = config.formats_repo

    from freeports_dev.create_test_page import (
        get_page_dict,
        print_pdf_line_sets,
        print_pdf_blks_table_MD,
        print_pdf_blks_table_ASCII,
    )
    from freeports.core import Algorithm
    from freeports_dev.input_db import get_test_companies as gtc

    base_path = repo / "tests" / "formats"
    report_file = args.report or (base_path / args.format / "report.pdf")

    from freeports.formats_repo import get_formats

    page = get_page_dict(str(report_file), args.page)

    # The reading modes come before the algorithm is loaded, and that ordering is the point: they
    # are what one uses on a document whose format does not exist yet, or does not load. Loading
    # first would make the tool refuse exactly the page one is trying to understand.
    if args.mode in ("structured", "semistructured", "unstructured"):
        if not args.strings:
            print("Error: --strings is required for line-set mode")
            sys.exit(1)
        print_pdf_line_sets(page, args.strings, mode=args.mode)
        return

    if args.mode in ("lines", "images"):
        _inspect_page_view(args, config, page, report_file)
        return

    a = Algorithm.load(repo, args.format, get_formats(repo))

    filter_data = args.filter_data
    if filter_data is None:
        filter_data = gtc(repo, config.target_lists, config)

    page_type = config.page_type
    if args.mode == "pdf_blks":
        pdf_blks = a.apply_pdf_extract(page, page_type)
        for blk in pdf_blks:
            print(blk)
    elif args.mode == "txt_blks":
        txt_blks = a.apply_text_filter(page, filter_data, page_type)
        for blk in txt_blks:
            print(blk)
    elif args.mode == "results":
        results = a.apply_deserialize(page, filter_data, page_type)
        for r in results:
            print(r)
    elif args.mode == "table_md":
        pdf_blks = a.apply_pdf_extract(page, page_type)
        print_pdf_blks_table_MD(pdf_blks)
    elif args.mode == "table_ascii":
        pdf_blks = a.apply_pdf_extract(page, page_type)
        print_pdf_blks_table_ASCII(pdf_blks)


def _inspect_page_view(args, config, page, report_file):
    """The two reading modes: the page's lines as a table, or its raster images as pictures.

    Both take the page as the engine reads it, so that what is shown is what a pipe would see.
    """
    import pymupdf

    from freeports.utils.pdf_extract import pdflines_from_pagedict
    from freeports_dev import page_view

    lines = pdflines_from_pagedict(page)

    if args.mode == "lines":
        shown = _selected(lines, args.select) if args.select else lines
        print(
            "\n".join(
                page_view.render_lines(
                    shown,
                    order=args.order,
                    columns=args.columns,
                    page_width=page.get("width"),
                    text_width=config.text_width,
                    codepoints=args.codepoints,
                )
            )
        )
        return

    document = pymupdf.Document(str(report_file))
    counts = page_view.document_image_counts(document) if args.scan_document else None
    rows = page_view.render_images(
        document[args.page - 1],
        page,
        lines,
        cols=config.preview_columns,
        dpi=args.dpi,
        document_counts=counts,
        save_dir=args.save_images,
        page_number=args.page,
    )
    print("\n".join(rows))


def _selected(lines, expression):
    """The lines a compact selection expression picks, in the syntax the CSV files already use."""
    from freeports.utils.pdf_extract import pdfline_selection_from_str

    return pdfline_selection_from_str(expression).select(lines)


def _cmd_find_text(args):
    """Where in a document a text occurs — the question ``inspect-page`` cannot be asked."""
    config = DevConfig(args)

    from freeports_dev import find_text

    try:
        documents = find_text.documents_to_search(
            [Path(p) for p in (args.paths or [])],
            repo=config.formats_repo,
            format_name=args.format,
        )
        match_text = find_text.text_predicate(
            text=args.text, regex=args.regex, ignore_case=not args.case_sensitive
        )
        pages = find_text.parse_pages(args.pages)
    except find_text.SearchError as error:
        print(f"Error: {error}")
        sys.exit(1)

    match_font = find_text.font_predicate(args.font)
    found = 0
    # Printed as they arrive rather than collected: a directory of six hundred reports is minutes
    # of reading, and the first hit is usually the one wanted.
    for hit in find_text.search(
        documents,
        match_text,
        match_font=match_font,
        pages=pages,
        max_hits=config.max_hits,
    ):
        found += 1
        if args.pages_only:
            print(f"{find_text.display_path(hit.document)}:{hit.page}")
        else:
            print(find_text.format_hit(hit, text_width=config.text_width))

    if not found:
        # Not an error: "it is not there" is a legitimate and frequent answer, and the search said
        # which documents it opened to find that out.
        print(f"no match in {len(documents)} document(s)")


def _tool_options_among_probe_words(args):
    """Take the tool's own options out of the words after the probe's name.

    ``freeports-dev probe run sfdr_title --summary a.pdf -F repo`` has to work: people add an option
    at the end of a command line. Whatever this parser does not recognise stays, in its order, for the
    probe -- which is how a probe's own ``--summary`` or ``--max-pages 30`` reaches it untouched.
    """
    parser = argparse.ArgumentParser(
        add_help=False, parents=[_common_parser(), _probes_dirs_parser()]
    )
    parser.add_argument("--format", "-f")
    found, remaining = parser.parse_known_args(args.words)
    for name in ("repo", "db_directory", "config", "format"):
        if getattr(found, name) is not None:
            setattr(args, name, getattr(found, name))
    if found.probes_dirs:
        args.probes_dirs = (args.probes_dirs or []) + found.probes_dirs
    args.words = remaining


def _cmd_probe(args):
    """List the ready-made probes, or run one on some documents."""
    if args.probe_command == "run":
        _tool_options_among_probe_words(args)
    config = DevConfig(args)

    from freeports_dev import find_text, probe

    try:
        probes = probe.discover(config.probes_dirs)
        if args.probe_command != "run":
            print(probe.format_listing(probes))
            sys.exit(0)
        chosen = probe.find_probe(probes, args.name)
        arguments, paths = probe.split_arguments(args.words)
        if not paths and not args.format:
            raise probe.ProbeError(
                "no documents: name PDFs or directories after the probe's arguments, or a format "
                "with --format"
            )
        documents = find_text.documents_to_search(
            paths, repo=config.formats_repo, format_name=args.format
        )
    except (probe.ProbeError, find_text.SearchError) as error:
        print(f"Error: {error}")
        sys.exit(1)
    sys.exit(probe.run(chosen, arguments, documents))


def _cmd_inspect_document(args):
    repo = DevConfig(args).formats_repo

    from freeports.core import Algorithm
    from freeports.formats_repo import get_formats
    import pymupdf

    base_path = repo / "tests" / "formats"
    report_file = args.report or (base_path / args.format / "report.pdf")

    a = Algorithm.load(repo, args.format, get_formats(repo))
    pdf_file = pymupdf.Document(str(report_file))

    # The whole document is classified even when only one page is asked about, and that is not
    # waste. A format may supply a finalizer that rewrites the raw per-page answers looking at all
    # of them at once -- "every page after the holdings header is holdings" -- so a page classified
    # on its own can get a different answer from the same page classified in its document. The
    # isolated answer is the wrong one, and it is the one somebody would act on.
    pages = [p.get_text("dict") for p in pdf_file]
    classifications = a.classify_pages(pages)

    if args.page is not None:
        if not 1 <= args.page <= len(classifications):
            print(
                f"Error: page {args.page} is outside {report_file}, which has {len(classifications)}"
            )
            sys.exit(1)
        numbered = [(args.page, classifications[args.page - 1])]
    else:
        numbered = list(enumerate(classifications, 1))

    for number, page_class in numbered:
        print(
            f"Page {number}: {page_class if page_class is not None else 'unclassified'}"
        )


def _cmd_init_repo(args):
    from freeports_dev.repo_init import init_format_repo

    target = Path(args.path).resolve()
    init_format_repo(target, quiet=args.quiet)


def _cmd_init_input_db(args):
    from freeports_dev.repo_init import init_input_db

    init_input_db(Path(args.path).resolve(), sample=args.sample, quiet=args.quiet)


def _cmd_setup_input_db(args):
    repo = DevConfig(args).formats_repo
    from freeports_dev.input_db import copy_default_input_db

    copy_default_input_db(repo / "tests")
    print(f"Input DB created at {repo / 'tests' / 'input_db'}")


def _env_flag(name):
    """A switch read from the environment, using the same words the engine's config accepts."""
    import os

    return (os.environ.get(name) or "").strip().lower() in (
        "1",
        "true",
        "yes",
        "y",
        "t",
        "on",
    )


def _ci_root(args):
    """The repository being gated: ``--repo`` if given, otherwise the working directory.

    Deliberately *not* :attr:`DevConfig.formats_repo`. That setting answers "which formats
    repository do the development commands work on", and a configuration file naming one is exactly
    what an input database's commit hook must not pick up: it would gate one repository using
    another repository's root. The gate runs where the commit is being made, and that is the
    working directory unless somebody says otherwise on the command line.
    """
    return (
        Path(args.repo).expanduser().resolve()
        if getattr(args, "repo", None)
        else Path.cwd()
    )


#: What each class does at commit time, said in the place a person goes when a hook surprises them.
_CLASS_MEANING = {
    "prod": "a missed threshold, a failing suite or an unmeasured figure refuses the commit",
    "dev": "everything is measured and reported; nothing refuses the commit",
    "off": "the hook does nothing at all here",
}


def _cmd_branch_class(args):
    from freeports_dev.ci.config import CiConfig, ConfigError

    try:
        config = CiConfig(_ci_root(args), args)
        resolved = config.branch_class()
    except ConfigError as exc:
        print(f"Error: {exc}")
        sys.exit(2)

    if args.format == "json":
        import json

        print(
            json.dumps(
                {
                    "branch": resolved.branch,
                    "class": resolved.name,
                    "reason": resolved.reason,
                    "repo_kind": config.repo_kind,
                    "ci_file": str(config.path) if config.exists else None,
                },
                indent=2,
            )
        )
        return

    where = f"branch {resolved.branch}" if resolved.branch else "no branch checked out"
    print(f"{where} -> {resolved.name}")
    print(f"  because {resolved.reason}")
    if not config.exists:
        print(f"  ({config.path.name} is absent, so every default applies)")
    print(f"  {_CLASS_MEANING[resolved.name]}")


def _cmd_coverage(args):
    from freeports_dev.ci import formats as formats_coverage
    from freeports_dev.ci import report as ci_report
    from freeports_dev.ci.config import CiConfig, ConfigError

    root = _ci_root(args)
    if not (root / "metadata" / "formats.csv").exists():
        print(
            f"Error: {root} does not appear to be a formats repository "
            f"(missing metadata/formats.csv)"
        )
        sys.exit(1)

    try:
        CiConfig(root, args)
    except ConfigError as exc:
        print(f"Error: {exc}")
        sys.exit(2)

    coverage = formats_coverage.measure(root)
    head = ci_report.head_commit(root)

    if args.out is not None:
        # The measurements go where the run's output goes, never into the working directory.
        out = Path(args.out) if args.out else root / ci_report.REPORTS_DIR
        written = [
            measurement.write(
                (out / ci_report.file_name(measurement.metric))
                if out.is_dir() or not out.suffix
                else out
            )
            for measurement in coverage.measurements(head)
        ]
        for path in written:
            print(f"wrote {path}")
        if args.format == "none":
            return

    if args.format == "json":
        print(formats_coverage.render_json(coverage, head))
    elif args.format == "markdown":
        print(formats_coverage.render_markdown(coverage))
    elif args.format == "badges":
        print(formats_coverage.render_badges(coverage))
    elif args.format != "none":
        print(formats_coverage.render_text(coverage))


def _write_measurements(args, root, measurements):
    """Write each measurement into `reports/`, and say where it went.

    The output goes under the repository being measured, never into the working directory: a file
    a run produced belongs with the run's output, and a stray file in the directory somebody
    happened to be standing in is a bug, not a convenience.
    """
    from freeports_dev.ci import report as ci_report

    out = Path(args.out) if args.out else root / ci_report.REPORTS_DIR
    for measurement in measurements:
        path = measurement.write(out / ci_report.file_name(measurement.metric))
        print(f"wrote {path}")


def _python_roots(root):
    """Where a repository keeps the Python whose docstrings are being counted.

    The engine keeps it in `packages/<name>/src/<name>`; a formats repository keeps it in
    `content/`. Both are returned as `(metric, path)` so the aggregate and the per-package figures
    come out of one walk rather than one walk per name.
    """
    packages = root / "packages"
    if packages.is_dir():
        found = []
        for package in sorted(packages.iterdir()):
            source = package / "src" / package.name
            if source.is_dir():
                found.append((f"docs.python.{package.name}", source))
        return found
    if (root / "content").is_dir():
        return [("docs.python", root / "content")]
    return [("docs.python", root)]


def _cmd_doc_coverage(args):
    from freeports_dev.ci import docstrings
    from freeports_dev.ci import report as ci_report
    from freeports_dev.ci.config import CiConfig, ConfigError

    root = _ci_root(args)
    try:
        CiConfig(root, args)
    except ConfigError as exc:
        print(f"Error: {exc}")
        sys.exit(2)

    head = ci_report.head_commit(root)
    measurements = []
    objects = []
    for metric, source in _python_roots(root):
        coverage, measurement = docstrings.measure(source, metric, head)
        measurements.append(measurement)
        objects.extend(coverage.objects)

    if len(measurements) > 1:
        # The aggregate is recomputed over every object rather than averaged over the packages:
        # averaging would give a fifty-line package the same weight as a five-thousand-line one.
        whole = docstrings.DocstringCoverage(objects)
        measurements.insert(
            0,
            ci_report.Measurement(
                "docs.python",
                value=whole.ratio,
                unit="percent",
                head=head,
                breakdown={m.metric: m.value for m in measurements},
                detail={"documented": whole.documented, "total": whole.total},
            ),
        )

    if args.out is not None:
        _write_measurements(args, root, measurements)

    if args.format == "json":
        import json

        print(
            json.dumps(
                {m.metric: m.to_dict() for m in measurements}, indent=2, sort_keys=True
            )
        )
    elif args.format != "none":
        for measurement in measurements:
            detail = measurement.detail
            print(
                f"{measurement.metric:<34} {measurement.value:6.2f} %   "
                f"{detail.get('documented')}/{detail.get('total')}"
            )


def _cmd_lint_score(args):
    from freeports_dev.ci import lint
    from freeports_dev.ci import report as ci_report
    from freeports_dev.ci.config import CiConfig, ConfigError

    root = _ci_root(args)
    try:
        config = CiConfig(root, args)
    except ConfigError as exc:
        print(f"Error: {exc}")
        sys.exit(2)

    head = ci_report.head_commit(root)
    measurements = []

    if args.language in ("python", "both"):
        targets = args.paths or _default_python_lint_paths(root, config.repo_kind)
        result, why = lint.run_ruff(root, targets)
        measurements.append(
            result.measurement("lint.python", head)
            if result is not None
            else ci_report.Measurement.unmeasured("lint.python", why, "score", head)
        )

    if args.language in ("rust", "both"):
        crate = root / "packages" / "freeports"
        if not crate.is_dir():
            if args.language == "rust":
                print(f"Error: no crate at {crate}")
                sys.exit(1)
        else:
            result, why = lint.run_clippy(crate)
            measurements.append(
                result.measurement("lint.rust", head)
                if result is not None
                else ci_report.Measurement.unmeasured("lint.rust", why, "score", head)
            )

    if args.out is not None:
        _write_measurements(args, root, measurements)

    if args.format == "json":
        import json

        print(
            json.dumps(
                {m.metric: m.to_dict() for m in measurements}, indent=2, sort_keys=True
            )
        )
    elif args.format != "none":
        for measurement in measurements:
            if not measurement.is_measured:
                print(f"{measurement.metric:<14} NOT MEASURED   {measurement.reason}")
                continue
            detail = measurement.detail
            # The count prints beside the score, always: a score is what a threshold compares, a
            # count is what a person fixes.
            print(
                f"{measurement.metric:<14} {measurement.value:7.3f} / 10   "
                f"{detail.get('errors')} errors, {detail.get('warnings')} warnings "
                f"in {detail.get('statements')} lines"
            )


def _default_python_lint_paths(root, repo_kind):
    """What ruff is given when nobody says otherwise, per repository kind.

    The engine's own `Makefile` already decided this and wrote down why -- the test suites are
    linted too, because leaving them out is how a suite becomes the only unlinted code in a
    repository. This mirrors that list rather than inventing a second one.
    """
    from freeports_dev.ci import metrics

    if repo_kind == metrics.FORMATS:
        return ["content"]
    packages = root / "packages"
    if packages.is_dir():
        found = []
        for package in sorted(packages.iterdir()):
            for part in ("src", "tests"):
                if (package / part).is_dir():
                    found.append(str((package / part).relative_to(root)))
        return found or [str(root)]
    return None


def _cmd_ci_record(args):
    from freeports_dev.ci import readers
    from freeports_dev.ci import report as ci_report
    from freeports_dev.ci.readers import ReaderError

    root = _ci_root(args)
    if args.suite:
        _record_suites(args, root)
        return

    # Neither half is `required` in the parser, because there are two kinds of fact to record and
    # requiring the fields of one would make the other impossible to state. The check is here, and
    # it names the two shapes rather than the missing flag: somebody who wrote `--metric` alone has
    # not forgotten an argument, they have written half of one of two different commands.
    missing = [
        flag
        for flag, value in (
            ("--metric", args.metric),
            ("--from", args.source),
            ("--input", args.input),
        )
        if not value
    ]
    if missing:
        print(
            "Error: nothing to record. Either a measurement — --metric NAME --from TOOL --input "
            f"PATH, and {', '.join(missing)} {'is' if len(missing) == 1 else 'are'} missing — or a "
            "suite's outcome, --suite NAME:OUTCOME."
        )
        sys.exit(2)
    reader = readers.READERS[args.source]
    inputs = [Path(item).read_text(encoding="utf-8") for item in args.input]
    if len(inputs) > 1 and args.source != "coverage-py":
        print(
            f"Error: --input is repeatable only with --from coverage-py, not {args.source}"
        )
        sys.exit(2)
    payload = inputs if len(inputs) > 1 else inputs[0]
    try:
        measurement = reader(
            payload, metric=args.metric, head=ci_report.head_commit(root)
        )
    except ReaderError as exc:
        print(f"Error: {', '.join(args.input)} is not a {args.source} report: {exc}")
        sys.exit(2)

    out = Path(args.out) if args.out else root / ci_report.REPORTS_DIR
    path = measurement.write(
        out if out.suffix == ".json" else out / ci_report.file_name(measurement.metric)
    )
    _say_recorded(measurement, path)


def _say_recorded(measurement, path):
    """One line saying what was written, whether or not there was a figure to write.

    A figure nobody could compute has no value to format, and printing it used to crash with a
    `TypeError` from `None.__format__` — the one moment this command most needed to say something a
    person could act on, it produced a traceback instead. The reason the reader gave is printed in
    its place; the record itself is written either way, and says `unmeasured`.
    """
    if measurement.is_measured:
        told = f"{measurement.value:.2f} {measurement.unit or ''}"
    else:
        told = f"not measured — {measurement.reason}"
    print(f"{measurement.metric} = {told} -> {path}")


def _record_suites(args, root):
    """Write down that a suite ran here, and how it went.

    One file per suite, carrying the commit it ran at, for the reason every measurement carries
    one: the commit hook runs the fast suites now and reads the rest from the last full run, and
    without the commit beside the outcome there would be no way to tell a suite that passed on this
    code from one that passed a fortnight ago. `freeports-dev ci-check` then reports the difference
    as `stale`, and on a prod branch refuses it.
    """
    from freeports_dev.ci import report as ci_report
    from freeports_dev.ci import suites as ci_suites

    out = Path(args.out) if args.out else root / ci_report.REPORTS_DIR
    head = ci_report.head_commit(root)
    for item in args.suite:
        name, outcome = ci_suites.parse(item)
        if not name:
            print(f"Error: {item!r} names no suite. Write it as <suite>:<outcome>.")
            sys.exit(2)
        record = ci_report.SuiteOutcome(name, outcome, head=head)
        path = record.write(
            out if out.suffix == ".json" else out / ci_report.suite_file_name(name)
        )
        print(f"suite {name} = {outcome} -> {path}")


def _judge(args):
    """The gate over whatever is in ``reports/``, built the same way for whoever asks.

    Two commands need it — ``ci-check``, which chooses an exit status from it, and ``ci-report``,
    which writes it down — and two spellings of "what the gate found" would drift apart exactly
    where it matters most: the report would keep saying `passing` after the gate had started
    refusing. Exits 2 on a ``ci.yaml`` that cannot be obeyed, which is a different thing from a
    commit that failed the rules and is reported as such.
    """
    from freeports_dev.ci import gate as ci_gate
    from freeports_dev.ci import metrics
    from freeports_dev.ci import report as ci_report
    from freeports_dev.ci import suites as ci_suites
    from freeports_dev.ci.config import CiConfig, ConfigError

    root = _ci_root(args)
    reports_dir = (
        Path(args.reports)
        if getattr(args, "reports", None)
        else root / ci_report.REPORTS_DIR
    )
    try:
        config = CiConfig(root, args)
        measurements = ci_report.read_all(reports_dir)
        suite_outcomes = ci_report.read_suites(reports_dir)

        skipped = []
        if getattr(args, "skip_slow", False):
            # Loud, never the default, and it prints what it skipped: a figure that was not
            # checked, and a suite nobody ran, must not be mistaken afterwards for one that passed.
            # Suites are dropped from the table entirely rather than reported as `not run`, because
            # `--skip-slow` is a statement that the person is not asking about them at all.
            for metric in metrics.REGISTRY:
                if metric.cost == metrics.SLOW and not metric.is_family:
                    skipped.append(metric.pattern)
                    measurements.pop(metric.pattern, None)
            for suite in ci_suites.REGISTRY:
                if suite.cost == metrics.SLOW:
                    skipped.append(suite.name)
                    suite_outcomes.pop(suite.name, None)

        reported = dict(
            ci_gate.parse_suite(item) for item in (getattr(args, "suite", None) or [])
        )
        gate = ci_gate.evaluate(
            config,
            measurements,
            head=ci_report.head_commit(root),
            skipped_slow=skipped,
            suite_outcomes=suite_outcomes,
            reported_suites=reported,
        )
    except ConfigError as exc:
        print(f"Error: {exc}")
        sys.exit(2)

    for name in skipped:
        for verdict in gate.verdicts:
            if verdict.metric == name and verdict.state == ci_gate.UNMEASURED:
                verdict.state = ci_gate.NO_THRESHOLD
                verdict.reason = None
    if skipped:
        gate.conditions = [c for c in gate.conditions if c.name not in set(skipped)]

    return config, gate


def _cmd_ci_check(args):
    from freeports_dev.ci import gate as ci_gate

    _config, gate = _judge(args)
    print(
        ci_gate.render_json(gate)
        if args.format == "json"
        else ci_gate.render_text(gate)
    )
    sys.exit(gate.exit_status)


def _write_rendering(model, style, table, out):
    """One rendering of an already-built model to one path. Returns what to print, or exits.

    Shared by the single-rendering path and by `--render`, so the two cannot drift: a hook that
    wrote its README through one code path and its badges through another would eventually
    disagree with itself about what "rewrite between the markers" means.
    """
    from freeports_dev.ci import render

    if style == "badges":
        if out is None:
            print(
                "Error: badges are several files, so the destination must be a directory"
            )
            sys.exit(2)
        out.mkdir(parents=True, exist_ok=True)
        written = render.render_badges(model)
        for name, content in written.items():
            (out / name).write_text(content, encoding="utf-8")
        return f"{len(written)} badge files -> {out}"

    try:
        text = render.render(model, style, table)
    except KeyError as refused:
        print(f"Error: {refused.args[0]}")
        sys.exit(2)

    if out is None:
        sys.stdout.write(text)
        return None

    # Markdown and reStructuredText are rewritten *into* a file rather than over it: the table is
    # part of a page somebody wrote, and the rest of that page is theirs.
    if style in render.MARKERS:
        try:
            updated = render.rewrite(out.read_text(encoding="utf-8"), text)
        except OSError as exc:
            print(f"Error: {out} cannot be read: {exc}")
            sys.exit(1)
        except render.MarkersMissing as missing:
            print(f"Error: {out}: {missing}")
            sys.exit(1)
        out.write_text(updated, encoding="utf-8")
    else:
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(text, encoding="utf-8")
    return f"{style} -> {out}"


def _render_many(model, specifications):
    """Every rendering a caller asked for, from one model, in one process.

    The commit hook wants six of these — badges, the README block, three documentation pages and an
    HTML page — and used to get them by starting this command six times. Each start is an
    interpreter, and six of them were most of a second out of a commit gate budgeted at five: two
    thirds of the cost of publishing the report was `fork`, not rendering.

    It also removes the temporary file. The reason a hook built a model into `mktemp` and passed
    `--model` to every rendering was that six evaluations could disagree with each other; one
    process rendering six times cannot, and needs no file to guarantee it.

    A specification is `<format>[@<table>]:<path>`, the same vocabulary as `--format` and `--table`.
    A malformed one is refused rather than skipped: a hook that silently wrote five of the six
    things it was asked for would publish a report half of which describes an older run.
    """
    from freeports_dev.ci import render

    for specification in specifications:
        head, separator, path = specification.partition(":")
        style, _, table = head.partition("@")
        if not separator or not path or style not in render.FORMATS:
            print(
                f"Error: {specification!r} is not a rendering. Write it as "
                f"<format>[@<table>]:<path>, with the format one of {', '.join(render.FORMATS)}."
            )
            sys.exit(2)
        if table and table not in render.TABLES:
            print(
                f"Error: {specification!r} names no table. Try {', '.join(render.TABLES)}."
            )
            sys.exit(2)
        said = _write_rendering(
            model, style, table or render.DEFAULT_TABLE, Path(path).expanduser()
        )
        if said:
            print(said)


def _cmd_ci_report(args):
    """Write the gate run down, and never refuse anything over it.

    The exit status says whether this command could do what it was asked, and nothing about what it
    found: a hook that could be stopped by its own report is a hook people remove, which is the same
    rule the grants report already follows.
    """
    import json

    from freeports_dev.ci import render

    if args.model:
        # One evaluation rendered several times, which is what a hook does: five renderings that
        # each re-read `reports/` could disagree with each other if a measurement landed in between.
        #
        # Deliberately not a fallback: a `--model` naming a file that is not there is an error, not
        # a reason to evaluate the repository instead. A run that quietly answered a different
        # question from the one asked is the failure this whole pipeline exists to avoid.
        try:
            raw = (
                sys.stdin.read()
                if args.model == "-"
                else Path(args.model).read_text(encoding="utf-8")
            )
            model = json.loads(raw)
        except (OSError, ValueError) as exc:
            print(f"Error: {args.model} is not a model this command wrote: {exc}")
            sys.exit(2)
    else:
        config, gate = _judge(args)
        model = render.build(gate, config)

    if args.render:
        _render_many(model, args.render)
        return

    said = _write_rendering(
        model,
        args.format,
        args.table,
        Path(args.out).expanduser() if args.out else None,
    )
    if said:
        print(said)


def _ask(question, default_yes=True):
    """Ask on the terminal, reopening it when stdin is a hook's pipe rather than a person.

    **A prompt nobody can answer must never be read as a yes.** A rebase, a script, an editor's
    commit button and a CI runner all reach this with no terminal at all, and in each of those the
    honest answer is no -- the person who would have said yes is not there. Declined and
    unanswerable are the same answer, and the caller treats them the same way.
    """
    import io
    import os

    try:
        terminal = (
            io.open(os.dup(sys.stdin.fileno()), "r")
            if sys.stdin.isatty()
            else open("/dev/tty")
        )
    except (OSError, ValueError, io.UnsupportedOperation):
        return None
    try:
        suffix = "[Y/n]" if default_yes else "[y/N]"
        print(f"{question} {suffix} ", end="", flush=True)
        answer = terminal.readline().strip().lower()
    except (OSError, KeyboardInterrupt):
        return None
    finally:
        terminal.close()
    if not answer:
        return default_yes
    return answer in ("y", "yes")


def _cmd_fingerprint(args):
    from freeports_dev.ci import fingerprint as fp
    from freeports_dev.ci.config import CiConfig, ConfigError
    from freeports_dev.ci.manifest import ManifestError

    root = _ci_root(args)
    try:
        config = CiConfig(root, args)
        branch_class = config.branch_class()
    except ConfigError as exc:
        print(f"Error: {exc}")
        sys.exit(2)

    try:
        manifest, check = fp.check(root, config.repo_kind)
    except ManifestError as exc:
        print(f"Error: {exc}")
        sys.exit(2)

    if check is None:
        print(
            f"{root} declares no fingerprint of its own, so there is nothing to check here."
        )
        return

    if args.format == "json":
        import json

        proposal = check.proposal()
        print(
            json.dumps(
                {
                    "repo_kind": config.repo_kind,
                    "ok": check.ok,
                    "version": check.version_now,
                    "version_committed": check.version_committed,
                    "version_moved": check.version_moved,
                    "proposal": proposal[1] if proposal else None,
                    "proposal_kind": proposal[0] if proposal else None,
                    "fingerprints": [
                        {
                            "name": entry["name"],
                            "computed": entry["computed"],
                            "declared": entry["declared"],
                            "committed": entry["committed"],
                            "moved": entry["computed"] != entry["committed"],
                        }
                        for entry in check.entries
                    ],
                },
                indent=2,
                sort_keys=True,
            )
        )
        sys.exit(0 if check.ok else 1)

    for entry in check.entries:
        moved = entry["computed"] != entry["committed"]
        print(f"{entry['name']:<12} {'CHANGED' if moved else 'unchanged'}")
        if moved:
            print(f"  committed  {entry['committed'] or '(nothing at HEAD)'}")
            print(f"  computed   {entry['computed']}")

    if not check.moved:
        print(
            f"\nNothing a fingerprint covers has moved. {'.'.join(check.version_field)} stands."
        )
        _maybe_write(args, manifest, check)
        return

    print(
        f"\nversion     {check.version_now}   (at HEAD: {check.version_committed or 'nothing'})"
    )

    if check.version_moved:
        print(
            "The version has moved too, so the manifest's claim about its contents is true."
        )
        _maybe_write(args, manifest, check)
        return

    # The fingerprint moved and the version did not. The manifest would otherwise claim that this
    # version covers content it does not, which is a false statement, and the point of the field is
    # that it is not one.
    proposal = check.proposal()
    accepted = False
    if proposal and (args.update or args.propose):
        kind, next_version = proposal
        print(
            f"\n{', '.join(sorted(check.changed_directories))} moved, so this is a {kind} bump: "
            f"{check.version_now} -> {next_version}"
        )
        answer = True if args.noconfirm else _ask(f"Set the version to {next_version}?")
        if answer:
            manifest.set(check.version_field, next_version)
            for entry in check.entries:
                manifest.set(entry["keys"], entry["computed"])
            manifest.write()
            print(f"wrote {manifest.path}")
            accepted = True
        elif answer is None:
            print(
                "No terminal to ask on, so the answer is no — a prompt nobody can answer is not a yes."
            )
        else:
            print("Declined.")
    elif not proposal:
        print(
            f"\nBump {'.'.join(check.version_field)} by hand: only you know whether this is a "
            f"correction or a new format."
        )

    if accepted:
        return

    if branch_class.refuses:
        print(
            f"\nRefused: on a {branch_class.name} branch the version must move with the content it "
            f"covers."
        )
        sys.exit(1)
    print(
        f"\nOn a {branch_class.name} branch this is a warning. The new fingerprint is NOT written: "
        f"writing it would leave the manifest claiming that {check.version_now} covers content it "
        f"does not."
    )


def _maybe_write(args, manifest, check):
    """Write the computed fingerprints when the manifest is already entitled to hold them."""
    if not args.update:
        return
    changed = False
    for entry in check.entries:
        if entry["declared"] != entry["computed"]:
            manifest.set(entry["keys"], entry["computed"])
            changed = True
    if changed:
        manifest.write()
        print(f"wrote {manifest.path}")


def _ci_parser():
    """The options every gating subcommand accepts, in the three tiers the engine already uses.

    ``--min metric=value`` rather than one flag per metric: the set of metrics grows, and the
    per-package minima make it combinatorial -- there is no finite list of flags to write.
    """
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument(
        "--repo",
        "-r",
        dest="repo",
        metavar="PATH",
        help="Repository to gate [default: the working directory]",
    )
    parser.add_argument(
        "--branch-class",
        dest="branch_class",
        choices=["prod", "dev", "off"],
        help="Force the branch class [default: $FREEPORTS_CI_BRANCH_CLASS, then the `branches` "
        "map in ci.yaml, then dev]",
    )
    parser.add_argument(
        "--min",
        dest="min",
        action="append",
        metavar="METRIC=VALUE",
        help="Override one minimum, repeatable [default: $FREEPORTS_CI_MIN_<METRIC>, then the "
        "`thresholds` map in ci.yaml, then none -- an unconfigured metric is reported, never gated]",
    )
    parser.add_argument(
        "--keyserver",
        dest="keyserver",
        metavar="URL",
        help="Key server for the granters' fingerprints [default: $FREEPORTS_VALIDATE_KEYSERVER, "
        "then `keyserver` in ci.yaml, then https://keys.openpgp.org]",
    )
    return parser


def _common_parser():
    """The options every subcommand that works on an existing repository accepts.

    Declared once and inherited, so the repository is named the same way whichever subcommand is
    being run. The long names and short letters are the engine's: `freeports` accepts
    `--formats-directory`/`--repo`/`-F`/`-r` and `--db-directory`/`-I` for the same two things, and a
    format author should not have to remember which command wanted which spelling.
    """
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument(
        "--repo",
        "-r",
        "--formats-directory",
        "-F",
        dest="repo",
        metavar="PATH",
        help="Formats repository [default: $FREEPORTS_FORMATS_REPO_PATH, then `formats_repo` in the "
        "configuration file, then the working directory]",
    )
    parser.add_argument(
        "--db-directory",
        "-I",
        dest="db_directory",
        metavar="PATH",
        help="Input database, overriding the repository's own tests/input_db "
        "[default: $FREEPORTS_INPUT_DB_PATH]",
    )
    parser.add_argument(
        "--config",
        metavar="PATH",
        help="Configuration file to read [default: $FREEPORTS_CONFIG_FILE, then the file the engine "
        "would find in the working, user and system tiers]",
    )
    return parser


def _targets_parser():
    """The target lists, for the subcommands that actually filter by company."""
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument(
        "--target-list",
        "-T",
        dest="target_list",
        nargs="+",
        metavar="NAME",
        help="Lists to search [default: $FREEPORTS_DEV_TARGET_LIST, then `dev.target_lists` in the "
        "configuration file, then TEST]",
    )
    return parser


def _page_type_parser():
    """The page type, defaulted through the configuration rather than hard-coded in the flag."""
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument(
        "--page-type",
        "-t",
        dest="page_type",
        metavar="TYPE",
        help="Page type [default: $FREEPORTS_DEV_PAGE_TYPE, then `dev.page_type` in the "
        "configuration file, then investments]",
    )
    return parser


def _probes_dirs_parser():
    """Directories of one's own probes, searched before the ones the tool ships."""
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument(
        "--probes-dir",
        dest="probes_dirs",
        action="append",
        metavar="PATH",
        help="A directory of probes of your own, searched before the shipped ones; repeatable "
        "[default: $FREEPORTS_DEV_PROBES_DIRS, then `dev.probes_dirs` in the configuration file]",
    )
    return parser


def main():
    parser = argparse.ArgumentParser(
        prog="freeports-dev",
        description="Development tools for freeports format repositories",
    )
    sub = parser.add_subparsers(dest="command")
    common = _common_parser()
    targets = _targets_parser()
    page_type = _page_type_parser()

    p_test = sub.add_parser(
        "test", parents=[common, targets], help="Run format tests via pytest"
    )
    p_test.add_argument(
        "--format", "-f", help="Run tests only for a specific format (e.g. AMUNDI-EN24)"
    )
    # **The default is the fast half**, and it is the same default `make test` has.
    #
    # It used to be "everything", which made the two ways of running the suite in the same
    # repository mean two different things -- the one place they must agree. The fast half is the
    # loop format development actually runs in, and a run that leaves the other half out says so
    # in its last line rather than leaving it to be discovered.
    selection = p_test.add_mutually_exclusive_group()
    selection.add_argument(
        "--fast",
        dest="select",
        action="store_const",
        const="fast",
        help="Only the per-page tests, which is `make test-fast` [the default]",
    )
    selection.add_argument(
        "--slow",
        dest="select",
        action="store_const",
        const="slow",
        help="Only the whole-document tests, a full extraction run each — `make test-slow`",
    )
    selection.add_argument(
        "--all",
        dest="select",
        action="store_const",
        const="all",
        help="Both halves — `make test-all`",
    )
    p_test.set_defaults(select="fast")
    p_test.add_argument(
        "pytest_args", nargs=argparse.REMAINDER, help="Arguments forwarded to pytest"
    )

    p_make = sub.add_parser(
        "make-tests",
        parents=[common, targets, page_type],
        help="Create test fixtures for a format page",
    )
    p_make.add_argument(
        "--format", "-f", required=True, help="Format name (e.g. AMUNDI-EN24)"
    )
    p_make.add_argument("--page", "-p", type=int, required=True, help="Page number")
    p_make.add_argument(
        "--document", "-d", help="Document variant (for multi-document formats)"
    )
    p_make.add_argument("--report", help="Path to report PDF (defaults to test dir)")
    p_make.add_argument("--filter-data", help="Path to filter data .pkl file")
    p_make.add_argument(
        "--noconfirm", action="store_true", help="Skip confirmation prompts"
    )
    p_make.add_argument(
        "--noprint_results", action="store_true", help="Suppress result printing"
    )
    p_make.add_argument(
        "--print_txt_blks", action="store_true", help="Activate text blocks printing"
    )
    p_make.add_argument(
        "--print_pdf_blks", action="store_true", help="Activate pdf blocks printing"
    )
    p_make.add_argument("--skip-pdf-blks", action="store_true")
    p_make.add_argument("--skip-txt-blks", action="store_true")
    p_make.add_argument("--skip-results", action="store_true")

    p_page = sub.add_parser(
        "inspect-page",
        parents=[common, targets, page_type],
        help="Inspect a PDF page for format development",
    )
    p_page.add_argument("--format", "-f", required=True, help="Format name")
    p_page.add_argument("--page", "-p", type=int, required=True, help="Page number")
    p_page.add_argument(
        "--mode",
        "-m",
        default="results",
        choices=[
            "structured",
            "semistructured",
            "unstructured",
            "lines",
            "images",
            "pdf_blks",
            "txt_blks",
            "results",
            "table_md",
            "table_ascii",
        ],
        help=(
            "Inspection mode. "
            "Line-set modes (structured|semistructured|unstructured): "
            "search --strings on the page and print PdfLineSelection matching lines. "
            "Reading modes (lines|images): print every line of the page as a table with a font "
            "legend, or its raster images with their digests and an ASCII preview of the "
            "rendered region. Neither loads the format, so both work on a format that does not "
            "exist yet. "
            "Pipeline modes (pdf_blks|txt_blks|results): "
            "print output from the pdf_extract, text_filter, or full pipeline stage. "
            "Table modes (table_md|table_ascii): "
            "render pdf_blks with table-row and col-row metadata as a table"
        ),
    )
    p_page.add_argument(
        "--strings",
        nargs="*",
        help="Strings to search in the page (required for structured|semistructured|unstructured modes)",
    )
    p_page.add_argument("--report", help="Path to report PDF")
    p_page.add_argument(
        "--filter-data",
        help="Path to filter data .pkl file (for txt_blks and results modes)",
    )
    p_page.add_argument(
        "--order",
        choices=["y", "x"],
        default="y",
        help=(
            "lines mode: reading order — 'y' top to bottom then left to right, 'x' down each "
            "column then right [default: y]"
        ),
    )
    p_page.add_argument(
        "--columns",
        type=int,
        default=1,
        help=(
            "lines mode: split the page into this many vertical bands and finish each before "
            "the next — what a page carrying two tables side by side needs [default: 1]"
        ),
    )
    p_page.add_argument(
        "--select",
        help="lines mode: restrict to a compact line selection, e.g. 'Arial-BoldMT \"Total\"'",
    )
    p_page.add_argument(
        "--codepoints",
        action="store_true",
        help="lines mode: show non-ASCII characters as <U+XXXX>, which is what tells a box glyph from a letter",
    )
    p_page.add_argument(
        "--scan-document",
        action="store_true",
        help="images mode: also count each image digest across the whole document",
    )
    p_page.add_argument(
        "--save-images",
        metavar="DIR",
        help="images mode: write both the stored bytes and the rendered region of each image into DIR",
    )
    p_page.add_argument(
        "--dpi",
        type=int,
        default=150,
        help="images mode: resolution the page region is rendered at [default: 150]",
    )

    p_page.add_argument(
        "--text-width",
        type=int,
        help="lines mode: truncate the text column to this many characters [setting: dev.text_width]",
    )
    p_page.add_argument(
        "--preview-columns",
        type=int,
        help="images mode: width of the ASCII preview, in characters [setting: dev.preview_columns]",
    )

    p_find = sub.add_parser(
        "find-text",
        parents=[common],
        help="Find which pages of a document contain a text",
    )
    p_find.add_argument(
        "paths",
        nargs="*",
        help="PDFs or directories to search; a directory is walked to the bottom",
    )
    p_find.add_argument(
        "--format",
        "-f",
        help="Search the reports this format's tests already pin, instead of naming paths",
    )
    p_find.add_argument(
        "--text", help="Substring to look for (not a regular expression)"
    )
    p_find.add_argument(
        "--regex",
        help=(
            "Regular expression to look for. Allowed here and not in a PdfLineSelection: a search "
            "combines with nothing, a selection has to be comparable with other selections"
        ),
    )
    p_find.add_argument(
        "--case-sensitive",
        action="store_true",
        help="Match case, which is off by default",
    )
    p_find.add_argument("--font", help="Only lines whose font name contains this")
    p_find.add_argument(
        "--pages",
        help="Restrict to a one-based inclusive range: '7', '10-20', '900-', '-40'",
    )
    p_find.add_argument(
        "--pages-only",
        action="store_true",
        help="Print only 'document:page', which is what the other commands take",
    )
    p_find.add_argument(
        "--max-hits",
        type=int,
        help="Stop after this many hits [setting: dev.max_hits, default 50]",
    )
    p_find.add_argument(
        "--text-width",
        type=int,
        help="Truncate the printed text to this many characters [setting: dev.text_width]",
    )

    p_probe = sub.add_parser(
        "probe",
        help="List or run the ready-made probes: small scripts that ask one question of a document",
    )
    probe_sub = p_probe.add_subparsers(dest="probe_command")
    probe_dirs = _probes_dirs_parser()
    probe_sub.add_parser(
        "list",
        parents=[common, probe_dirs],
        help="Name each probe and the question it asks",
    )
    p_probe_run = probe_sub.add_parser(
        "run",
        parents=[common, probe_dirs],
        help="Run a probe: freeports-dev probe run [options] NAME [ARGUMENT...] PDF-OR-DIR...",
    )
    p_probe_run.add_argument(
        "--format",
        "-f",
        help="Also run on the reports this format's tests already pin",
    )
    p_probe_run.add_argument(
        "name", help="The probe, with or without the probe_ prefix"
    )
    p_probe_run.add_argument(
        "words",
        nargs=argparse.REMAINDER,
        metavar="ARGUMENT... DOCUMENT...",
        help="The probe's own arguments first, then PDFs or directories",
    )

    p_doc = sub.add_parser(
        "inspect-document",
        parents=[common],
        help="Classify pages of a PDF document to determine their page types",
    )
    p_doc.add_argument("--format", "-f", required=True, help="Format name")
    p_doc.add_argument(
        "--page",
        "-p",
        type=int,
        help="Specific page to classify (default: classify all pages)",
    )
    p_doc.add_argument("--report", help="Path to report PDF")

    p_init = sub.add_parser(
        "init-format-repo", help="Initialize a new format repository"
    )
    p_init.add_argument("path", help="Path for the new repository")
    # Without this the command can only be run by a person sitting at a terminal, which also means
    # it can never be checked by anything: `make distcheck` installs the built wheel and asks it to
    # create a repository, and an `input()` with nothing on stdin ends that in an EOFError.
    p_init.add_argument(
        "--quiet",
        "-q",
        action="store_true",
        help="Ask nothing and take the default answer (yes) to every prompt",
    )

    p_init_db = sub.add_parser("init-input-db", help="Initialize a new input database")
    p_init_db.add_argument("path", help="Path for the new input database")
    p_init_db.add_argument(
        "--sample",
        action="store_true",
        help="Fill the tables with the packaged example database (list TEST) instead of leaving "
        "them empty",
    )
    p_init_db.add_argument(
        "--quiet",
        "-q",
        action="store_true",
        help="Ask nothing and take the default answer (yes) to every prompt",
    )

    sub.add_parser(
        "setup-input-db",
        parents=[common],
        help="Create tests/input_db/ with default TEST list",
    )

    ci = _ci_parser()

    p_branch = sub.add_parser(
        "branch-class",
        parents=[ci],
        help="Print the class of the current branch and the rule that produced it",
    )
    p_branch.add_argument(
        "--format",
        choices=["text", "json"],
        default="text",
        help="Rendering [default: text]",
    )

    p_coverage = sub.add_parser(
        "coverage",
        parents=[ci],
        help="Measure how much of a formats repository is tested, by document",
    )
    p_coverage.add_argument(
        "--format",
        choices=["text", "json", "markdown", "badges", "none"],
        default="text",
        help="Rendering [default: text]",
    )
    p_coverage.add_argument(
        "--out",
        nargs="?",
        const="",
        metavar="PATH",
        help="Also write the measurements as JSON [default: reports/ under the repository]",
    )

    p_docs = sub.add_parser(
        "doc-coverage",
        parents=[ci],
        help="Measure what fraction of the public Python objects carry a docstring",
    )
    p_docs.add_argument(
        "--format",
        choices=["text", "json", "none"],
        default="text",
        help="Rendering [default: text]",
    )
    p_docs.add_argument(
        "--out",
        nargs="?",
        const="",
        metavar="PATH",
        help="Also write the measurements as JSON [default: reports/ under the repository]",
    )

    p_lint = sub.add_parser(
        "lint-score",
        parents=[ci],
        help="Score a linter's findings out of ten, with pylint's formula",
    )
    p_lint.add_argument(
        "--language",
        choices=["python", "rust", "both"],
        default="python",
        help="Which linter to run [default: python]",
    )
    p_lint.add_argument(
        "paths",
        nargs="*",
        help="Paths to lint [default: the repository's own source and test directories]",
    )
    p_lint.add_argument(
        "--format",
        choices=["text", "json", "none"],
        default="text",
        help="Rendering [default: text]",
    )
    p_lint.add_argument(
        "--out",
        nargs="?",
        const="",
        metavar="PATH",
        help="Also write the measurements as JSON [default: reports/ under the repository]",
    )

    p_record = sub.add_parser(
        "ci-record",
        parents=[ci],
        help="Normalise a measurement tool's own JSON into a reports/ file",
    )
    p_record.add_argument("--metric", help="The metric this report answers for")
    p_record.add_argument(
        "--from",
        dest="source",
        choices=sorted(
            [
                "llvm-cov",
                "coverage-py",
                "rustdoc",
                "ruff",
                "clippy",
                "validate",
                "check-keys",
            ]
        ),
        help="Which tool produced the input",
    )
    p_record.add_argument(
        "--input",
        action="append",
        metavar="PATH",
        help="The tool's own report. Repeatable for coverage-py, whose reports are then combined "
        "over their counts rather than averaged over the packages",
    )
    p_record.add_argument(
        "--suite",
        action="append",
        metavar="NAME:OUTCOME",
        help="Record that a test suite ran here and how it went, e.g. --suite rust.unit:passed. "
        "Repeatable. Written with the commit it ran at, which is what lets `ci-check` tell a "
        "suite that passed on this code from one that passed a fortnight ago. "
        "`freeports-dev ci-check` lists every suite name this repository has",
    )
    p_record.add_argument(
        "--out",
        metavar="PATH",
        help="Where to write it [default: reports/ under the repository]",
    )

    p_check = sub.add_parser(
        "ci-check",
        parents=[ci],
        help="The verdict: read reports/, apply ci.yaml, and choose an exit status",
    )
    p_check.add_argument(
        "--reports",
        metavar="PATH",
        help="Where the measurements are [default: reports/ under the repository]",
    )
    p_check.add_argument(
        "--suite",
        action="append",
        metavar="NAME:OUTCOME",
        help="A suite that ran just now, repeatable, e.g. --suite rust.unit:passed. It outranks "
        "what that suite left in reports/, because a run in progress is more current than one on "
        "disk. Every other suite is still listed, from its recorded outcome or as `not run`",
    )
    p_check.add_argument(
        "--skip-slow",
        action="store_true",
        help="Do not check the slow metrics at all. Loud, never the default, and it prints what "
        "it skipped [also $FREEPORTS_CI_SKIP_SLOW]",
    )
    p_check.add_argument(
        "--format",
        choices=["text", "json"],
        default="text",
        help="Rendering [default: text]",
    )

    # Imported here rather than lazily inside the command, because the parser needs the names of
    # the formats and the tables to offer them: one list, in the module that implements them, and
    # no second copy in the help text to fall out of step with it.
    from freeports_dev.ci import render

    # The same three options `ci-check` takes, because it is the same evaluation: a report of a run
    # gated differently from the run itself would be a report of something that never happened.
    p_report = sub.add_parser(
        "ci-report",
        parents=[ci],
        help="Write the verdict down: badges, a README block, documentation pages, one HTML page",
    )
    p_report.add_argument(
        "--reports",
        metavar="PATH",
        help="Where the measurements are [default: reports/ under the repository]",
    )
    p_report.add_argument(
        "--suite",
        action="append",
        metavar="NAME:OUTCOME",
        help="A suite that ran just now, repeatable, e.g. --suite rust.unit:passed. It outranks "
        "what that suite left in reports/, because a run in progress is more current than one on "
        "disk. Every other suite is still listed, from its recorded outcome or as `not run`",
    )
    p_report.add_argument(
        "--skip-slow",
        action="store_true",
        help="Do not read the slow metrics at all. The report then says so, and its status is "
        "inconclusive rather than passing [also $FREEPORTS_CI_SKIP_SLOW]",
    )
    p_report.add_argument(
        "--format",
        "-f",
        choices=list(render.FORMATS),
        default="json",
        help="Rendering [default: json, which is the model every other one is drawn from]",
    )
    p_report.add_argument(
        "--table",
        "-t",
        choices=list(render.TABLES),
        default=render.DEFAULT_TABLE,
        help=f"Which arrangement, for markdown and rst [default: {render.DEFAULT_TABLE}]",
    )
    p_report.add_argument(
        "--render",
        action="append",
        metavar="FORMAT[@TABLE]:PATH",
        help="Write one rendering, repeatable: every one is drawn from a single evaluation in a "
        "single process, e.g. --render badges:ci/report/badges/ --render markdown:README.md "
        "--render rst@thresholds:docs/dev/thresholds.rst. This is what a commit hook wants; six "
        "invocations of this command are six interpreter starts and can disagree with each other",
    )
    p_report.add_argument(
        "--breakdown-limit",
        dest="breakdown_limit",
        metavar="N",
        help="How many breakdown lines per metric a document shows, worst first; 0 for all of them "
        f"[default: $FREEPORTS_CI_BREAKDOWN_LIMIT, then `report.breakdown_limit` in ci.yaml, then "
        f"{render.DEFAULT_BREAKDOWN_LIMIT}. --format json always carries every line]",
    )
    p_report.add_argument(
        "--model",
        "-M",
        metavar="PATH",
        help="Render a model written earlier instead of evaluating again, so one run can be "
        "rendered several times; `-` reads it from a pipe [write one with --format json]",
    )
    p_report.add_argument(
        "--out",
        "-o",
        metavar="PATH",
        help="Where to write it: a file, or the directory the badges go in. Markdown and rst are "
        "rewritten between their markers, not over the file [default: standard output]",
    )

    p_fingerprint = sub.add_parser(
        "fingerprint",
        parents=[ci],
        help="Check that the manifest's hash of its own contents moved with its version",
    )
    p_fingerprint.add_argument(
        "--check",
        action="store_true",
        help="Compare and report, changing nothing [the default]",
    )
    p_fingerprint.add_argument(
        "--update",
        action="store_true",
        help="Rewrite the manifest when it is entitled to hold the new value",
    )
    p_fingerprint.add_argument(
        "--propose",
        action="store_true",
        help="Offer the mechanical version bump interactively, without writing anything else",
    )
    p_fingerprint.add_argument(
        "--noconfirm",
        action="store_true",
        help="Accept the proposal without asking. Never use this in a hook",
    )
    p_fingerprint.add_argument(
        "--format",
        choices=["text", "json"],
        default="text",
        help="Rendering [default: text]",
    )

    args = parser.parse_args()

    # The third tier for the one flag that is a switch rather than a value. `--skip-slow` can only
    # ever turn the setting on, so a command line that does not mention it leaves an environment
    # that did alone.
    if getattr(args, "skip_slow", False) is False and _env_flag(
        "FREEPORTS_CI_SKIP_SLOW"
    ):
        args.skip_slow = True

    if args.command == "test":
        _cmd_test(args)
    elif args.command == "make-tests":
        _cmd_make_tests(args)
    elif args.command == "inspect-page":
        _cmd_inspect_page(args)
    elif args.command == "inspect-document":
        _cmd_inspect_document(args)
    elif args.command == "find-text":
        _cmd_find_text(args)
    elif args.command == "probe":
        _cmd_probe(args)
    elif args.command == "init-format-repo":
        _cmd_init_repo(args)
    elif args.command == "init-input-db":
        _cmd_init_input_db(args)
    elif args.command == "setup-input-db":
        _cmd_setup_input_db(args)
    elif args.command == "branch-class":
        _cmd_branch_class(args)
    elif args.command == "coverage":
        _cmd_coverage(args)
    elif args.command == "doc-coverage":
        _cmd_doc_coverage(args)
    elif args.command == "lint-score":
        _cmd_lint_score(args)
    elif args.command == "ci-record":
        _cmd_ci_record(args)
    elif args.command == "ci-check":
        _cmd_ci_check(args)
    elif args.command == "ci-report":
        _cmd_ci_report(args)
    elif args.command == "fingerprint":
        _cmd_fingerprint(args)
    else:
        parser.print_help()
        sys.exit(1)


if __name__ == "__main__":
    main()
