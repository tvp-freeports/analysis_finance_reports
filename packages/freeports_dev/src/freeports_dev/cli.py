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
    pytest_args.extend(extra)
    sys.exit(pytest.main(pytest_args))


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

    a = Algorithm.load(repo, args.format, get_formats(repo))
    page = get_page_dict(str(report_file), args.page)

    if args.mode in ("structured", "semistructured", "unstructured"):
        if not args.strings:
            print("Error: --strings is required for line-set mode")
            sys.exit(1)
        print_pdf_line_sets(page, args.strings, mode=args.mode)
        return

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
    print(
        f"{measurement.metric} = {measurement.value:.2f} {measurement.unit or ''} -> {path}"
    )


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
    from freeports_dev.ci.config import CiConfig, ConfigError

    root = _ci_root(args)
    try:
        config = CiConfig(root, args)
        measurements = ci_report.read_all(
            Path(args.reports)
            if getattr(args, "reports", None)
            else root / ci_report.REPORTS_DIR
        )

        skipped = []
        if getattr(args, "skip_slow", False):
            # Loud, never the default, and it prints what it skipped: a figure that was not
            # checked must not be mistaken afterwards for one that passed.
            for metric in metrics.REGISTRY:
                if metric.cost == metrics.SLOW and not metric.is_family:
                    skipped.append(metric.pattern)
                    measurements.pop(metric.pattern, None)

        conditions = [
            ci_gate.parse_suite(item) for item in (getattr(args, "suite", None) or [])
        ]
        gate = ci_gate.evaluate(
            config,
            measurements,
            conditions,
            head=ci_report.head_commit(root),
            skipped_slow=skipped,
        )
    except ConfigError as exc:
        print(f"Error: {exc}")
        sys.exit(2)

    for name in skipped:
        for verdict in gate.verdicts:
            if verdict.metric == name and verdict.state == ci_gate.UNMEASURED:
                verdict.state = ci_gate.NO_THRESHOLD
                verdict.reason = None

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

    out = Path(args.out).expanduser() if args.out else None

    if args.format == "badges":
        if out is None:
            print(
                "Error: badges are several files, so --out must name a directory for them"
            )
            sys.exit(2)
        out.mkdir(parents=True, exist_ok=True)
        written = render.render_badges(model)
        for name, content in written.items():
            (out / name).write_text(content, encoding="utf-8")
        print(f"{len(written)} badge files -> {out}")
        return

    try:
        text = render.render(model, args.format, args.table)
    except KeyError as refused:
        print(f"Error: {refused.args[0]}")
        sys.exit(2)

    if out is None:
        sys.stdout.write(text)
        return

    # Markdown and reStructuredText are rewritten *into* a file rather than over it: the table is
    # part of a page somebody wrote, and the rest of that page is theirs.
    if args.format in render.MARKERS:
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
    print(f"{args.format} -> {out}")


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
    p_record.add_argument(
        "--metric", required=True, help="The metric this report answers for"
    )
    p_record.add_argument(
        "--from",
        dest="source",
        required=True,
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
        required=True,
        action="append",
        metavar="PATH",
        help="The tool's own report. Repeatable for coverage-py, whose reports are then combined "
        "over their counts rather than averaged over the packages",
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
        help="A test suite's outcome, repeatable, e.g. --suite fast:passed",
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
        help="A test suite's outcome, repeatable, e.g. --suite fast:passed",
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
