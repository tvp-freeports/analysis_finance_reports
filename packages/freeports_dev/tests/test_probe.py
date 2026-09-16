"""Probes: ready-made questions to ask a document, shipped with the tool.

A probe is a script that answers one question about a PDF -- *what kind of document is this*, *is
there a management company, and what does the page say next to it*. It is what one writes before a
format exists, to find out whether an assumption holds on more than one report. The ones worth
keeping ship here, so that the next person or agent starting a format has them already.

Three decisions are held by these tests.

**A probe is an independent file.** No probe imports another, and the tool does not import any of
them: ``list`` reads the docstring without executing the file, and ``run`` starts it as a separate
process. Adding a probe is adding a file, and a broken one cannot break the command.

**One's own probes come first.** Directories named in the configuration are searched before the
shipped probes, so a probe being improved locally can stand in for the published one -- and ``list``
says when that happens, because a silent substitution is the one thing a probe must never do.

**Arguments first, documents last.** A probe's own parameters come before the PDFs, so the command
can tell them apart without knowing anything about any probe: the documents start at the first word
that names an existing file or directory.
"""

import sys
from argparse import Namespace
from pathlib import Path

import pytest

from freeports_dev.probe import (
    SHIPPED_DIR,
    ProbeError,
    command_for,
    discover,
    find_probe,
    split_arguments,
    summary_of,
)


def write_probe(directory, name, body='"""Ask something."""\n'):
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / f"probe_{name}.py"
    path.write_text(body)
    return path


class TestTheShippedProbes:
    """The first wave agreed with the user, each one a question any format author may ask."""

    FIRST_WAVE = {"doc_kind", "sfdr_title", "inv_managers", "manco", "assets"}

    def test_the_first_wave_is_shipped(self):
        assert self.FIRST_WAVE <= set(discover([]))

    def test_every_shipped_probe_describes_itself(self):
        for probe in discover([]).values():
            assert probe.summary and not probe.summary.startswith("("), probe.name

    def test_every_shipped_probe_says_how_to_call_it(self):
        for probe in discover([]).values():
            assert "Usage:" in probe.path.read_text(), probe.name

    def test_no_shipped_probe_imports_another(self):
        for probe in discover([]).values():
            source = probe.path.read_text()
            assert "import probe_" not in source and "from probe_" not in source, (
                probe.name
            )

    def test_the_directory_is_not_a_python_package(self):
        # Probes are files to run, not modules to import; an __init__ would invite the second use.
        assert not (SHIPPED_DIR / "__init__.py").exists()


class TestDescribingAProbeWithoutRunningIt:
    def test_the_summary_is_the_first_line_of_the_docstring(self, tmp_path):
        path = write_probe(tmp_path, "x", '"""Is it there?\n\nLonger text.\n"""\n')
        assert summary_of(path) == "Is it there?"

    def test_the_file_is_not_executed(self, tmp_path):
        path = write_probe(
            tmp_path, "boom", '"""Harmless to describe."""\nraise SystemExit("ran")\n'
        )
        assert summary_of(path) == "Harmless to describe."

    def test_a_probe_without_a_docstring_says_so(self, tmp_path):
        path = write_probe(tmp_path, "bare", "print('hi')\n")
        assert summary_of(path) == "(no description)"

    def test_a_probe_that_does_not_parse_is_listed_not_fatal(self, tmp_path):
        path = write_probe(tmp_path, "broken", "def (:\n")
        assert summary_of(path).startswith("(unreadable")


class TestFindingProbes:
    def test_the_name_is_the_file_name_without_prefix_and_suffix(self, tmp_path):
        write_probe(tmp_path, "cover_line")
        assert "cover_line" in discover([tmp_path])

    def test_only_files_named_probe_star_are_probes(self, tmp_path):
        write_probe(tmp_path, "real")
        (tmp_path / "helper.py").write_text('"""Not a probe."""\n')
        (tmp_path / "README.md").write_text("notes")
        found = {p.name for p in discover([tmp_path]).values() if p.origin == tmp_path}
        assert found == {"real"}

    def test_an_own_probe_hides_the_shipped_one_of_the_same_name_and_says_so(
        self, tmp_path
    ):
        mine = write_probe(tmp_path, "doc_kind", '"""My improved version."""\n')
        probe = discover([tmp_path])["doc_kind"]
        assert probe.path == mine
        assert probe.hides == SHIPPED_DIR / "probe_doc_kind.py"

    def test_among_own_directories_the_first_named_wins(self, tmp_path):
        first = write_probe(tmp_path / "a", "same")
        write_probe(tmp_path / "b", "same")
        assert discover([tmp_path / "a", tmp_path / "b"])["same"].path == first

    def test_a_missing_directory_is_an_error_rather_than_nothing(self, tmp_path):
        # A misspelled directory that silently contributed no probes would look like a probe that
        # does not exist, which is the wrong thing to go and fix.
        with pytest.raises(ProbeError, match="no such directory"):
            discover([tmp_path / "nowhere"])

    @pytest.mark.parametrize(
        "spelling", ["doc_kind", "probe_doc_kind", "probe_doc_kind.py"]
    )
    def test_a_probe_may_be_named_with_or_without_prefix_and_suffix(self, spelling):
        assert find_probe(discover([]), spelling).name == "doc_kind"

    def test_an_unknown_probe_names_the_ones_that_exist(self):
        with pytest.raises(ProbeError, match="doc_kind"):
            find_probe(discover([]), "does_not_exist")


class TestSeparatingArgumentsFromDocuments:
    def test_the_documents_start_at_the_first_existing_path(self, tmp_path):
        pdf = tmp_path / "a.pdf"
        pdf.write_bytes(b"%PDF-1.4")
        assert split_arguments(["en", str(pdf)]) == (["en"], [pdf])

    def test_a_directory_counts_as_a_document(self, tmp_path):
        assert split_arguments(["it", str(tmp_path)]) == (["it"], [tmp_path])

    def test_with_no_path_everything_is_an_argument(self):
        assert split_arguments(["it", "en"]) == (["it", "en"], [])

    def test_an_argument_after_the_first_document_is_an_error(self, tmp_path):
        pdf = tmp_path / "a.pdf"
        pdf.write_bytes(b"%PDF-1.4")
        with pytest.raises(ProbeError, match="after the documents"):
            split_arguments([str(pdf), "en"])


class TestTheCommandLineOfTheProcess:
    def test_the_probe_runs_with_the_interpreter_of_the_tool(self, tmp_path):
        path = write_probe(tmp_path, "x")
        probe = discover([tmp_path])["x"]
        pdf = tmp_path / "a.pdf"
        assert command_for(probe, ["en"], [pdf]) == [
            sys.executable,
            str(path),
            "en",
            str(pdf),
        ]


@pytest.fixture
def runs_a_process():
    """Marks a test that starts a real process: see ``conftest.SLOW_FIXTURES``."""


class TestRunningFromTheCommandLine:
    def run_cli(self, monkeypatch, *argv):
        from freeports_dev import cli

        monkeypatch.setattr(sys, "argv", ["freeports-dev", *argv])
        with pytest.raises(SystemExit) as exit_info:
            cli.main()
        return exit_info.value.code

    def test_the_exit_status_of_the_probe_is_the_exit_status_of_the_command(
        self, tmp_path, monkeypatch, capfd, runs_a_process
    ):
        write_probe(
            tmp_path,
            "echo",
            '"""Echo."""\nimport sys\nprint("args", sys.argv[1:])\nsys.exit(3)\n',
        )
        pdf = tmp_path / "a.pdf"
        pdf.write_bytes(b"%PDF-1.4")
        monkeypatch.setenv("FREEPORTS_CONFIG_FILE", "")
        code = self.run_cli(
            monkeypatch,
            "probe",
            "run",
            "--probes-dir",
            str(tmp_path),
            "echo",
            "en",
            str(pdf),
        )
        assert code == 3
        assert f"args ['en', '{pdf}']" in capfd.readouterr().out

    def test_list_marks_a_probe_that_hides_a_shipped_one(
        self, tmp_path, monkeypatch, capsys
    ):
        write_probe(tmp_path, "doc_kind", '"""Mine."""\n')
        monkeypatch.setenv("FREEPORTS_CONFIG_FILE", "")
        code = self.run_cli(monkeypatch, "probe", "list", "--probes-dir", str(tmp_path))
        out = capsys.readouterr().out
        assert code == 0
        assert "doc_kind" in out and "hides the shipped probe" in out

    def test_the_tools_own_options_may_follow_the_probe_name(
        self, tmp_path, monkeypatch, capfd, runs_a_process
    ):
        write_probe(
            tmp_path, "echo", '"""Echo."""\nimport sys\nprint("args", sys.argv[1:])\n'
        )
        pdf = tmp_path / "a.pdf"
        pdf.write_bytes(b"%PDF-1.4")
        monkeypatch.setenv("FREEPORTS_CONFIG_FILE", "")
        code = self.run_cli(
            monkeypatch,
            "probe",
            "run",
            "echo",
            "--summary",
            str(pdf),
            "--probes-dir",
            str(tmp_path),
        )
        assert code == 0
        assert f"args ['--summary', '{pdf}']" in capfd.readouterr().out

    def test_a_probe_option_with_a_value_keeps_its_value(
        self, tmp_path, monkeypatch, capfd, runs_a_process
    ):
        write_probe(
            tmp_path, "echo", '"""Echo."""\nimport sys\nprint("args", sys.argv[1:])\n'
        )
        pdf = tmp_path / "a.pdf"
        pdf.write_bytes(b"%PDF-1.4")
        monkeypatch.setenv("FREEPORTS_CONFIG_FILE", "")
        code = self.run_cli(
            monkeypatch,
            "probe",
            "run",
            "--probes-dir",
            str(tmp_path),
            "echo",
            "--max-pages",
            "30",
            str(pdf),
        )
        assert code == 0
        assert f"args ['--max-pages', '30', '{pdf}']" in capfd.readouterr().out

    def test_running_without_documents_is_an_error_that_says_how(
        self, tmp_path, monkeypatch, capsys
    ):
        monkeypatch.setenv("FREEPORTS_CONFIG_FILE", "")
        code = self.run_cli(monkeypatch, "probe", "run", "doc_kind")
        assert code == 1
        assert "--format" in capsys.readouterr().out


class TestTheDirectoriesSetting:
    """``dev.probes_dirs``, ``FREEPORTS_DEV_PROBES_DIRS``, ``--probes-dir``: the usual three tiers."""

    def config(self, **overrides):
        from freeports_dev.config import DevConfig

        return DevConfig(
            Namespace(
                **{"config": None, "repo": None, "db_directory": None, **overrides}
            )
        )

    def test_nothing_said_means_no_directory_of_ones_own(self, monkeypatch, tmp_path):
        monkeypatch.chdir(tmp_path)
        monkeypatch.delenv("FREEPORTS_DEV_PROBES_DIRS", raising=False)
        monkeypatch.setenv("FREEPORTS_CONFIG_FILE", "")
        assert self.config().probes_dirs == []

    def test_the_file_is_read_relative_to_the_file(self, monkeypatch, tmp_path):
        pytest.importorskip("freeports.cli")
        config_file = tmp_path / "conf" / "freeports-conf.yaml"
        config_file.parent.mkdir()
        config_file.write_text("dev:\n  probes_dirs: [lab, /abs/probes]\n")
        monkeypatch.delenv("FREEPORTS_DEV_PROBES_DIRS", raising=False)
        dirs = self.config(config=str(config_file)).probes_dirs
        assert dirs == [tmp_path / "conf" / "lab", Path("/abs/probes")]

    def test_the_environment_is_a_path_list_relative_to_the_working_directory(
        self, monkeypatch, tmp_path
    ):
        monkeypatch.chdir(tmp_path)
        monkeypatch.setenv("FREEPORTS_CONFIG_FILE", "")
        monkeypatch.setenv("FREEPORTS_DEV_PROBES_DIRS", f"a{__import__('os').pathsep}b")
        assert self.config().probes_dirs == [tmp_path / "a", tmp_path / "b"]

    def test_the_command_line_wins_over_the_environment(self, monkeypatch, tmp_path):
        monkeypatch.chdir(tmp_path)
        monkeypatch.setenv("FREEPORTS_CONFIG_FILE", "")
        monkeypatch.setenv("FREEPORTS_DEV_PROBES_DIRS", "from-env")
        assert self.config(probes_dirs=["from-flag"]).probes_dirs == [
            tmp_path / "from-flag"
        ]


class TestThePresentationSettingsHaveAFileTier:
    """``dev.text_width``, ``dev.preview_columns``, ``dev.max_hits`` -- the file tier the engine
    refused until the keys were added to its parser."""

    def test_the_three_are_read_from_the_file(self, monkeypatch, tmp_path):
        pytest.importorskip("freeports.cli")
        from freeports_dev.config import DevConfig

        for name in ("TEXT_WIDTH", "PREVIEW_COLUMNS", "MAX_HITS"):
            monkeypatch.delenv(f"FREEPORTS_DEV_{name}", raising=False)
        config_file = tmp_path / "freeports-conf.yaml"
        config_file.write_text(
            "dev:\n  text_width: 42\n  preview_columns: 33\n  max_hits: 7\n"
        )
        config = DevConfig(
            Namespace(config=str(config_file), repo=None, db_directory=None)
        )
        assert (config.text_width, config.preview_columns, config.max_hits) == (
            42,
            33,
            7,
        )
