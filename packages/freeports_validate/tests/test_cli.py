"""The settings merge: the one part of `freeports-validate` that is Python all the way down.

`cli.py` is the only place that reads the command line, the environment and the configuration file,
and everything downstream of it is a shell script reading environment variables. So the thing to
test here is not what a subcommand does -- it is *what the subcommand is told*, which is the
`execve` at the end of `main()`. These tests intercept that call and read the environment it was
about to hand over.

In process, rather than through :fixture:`run_validate`, deliberately. A merge has a truth table,
the table has a lot of rows, and each row is one dictionary lookup: paying for an interpreter and a
`gpg` agent per row would buy nothing, and the rows that matter most -- the ones where a tier is
*absent* -- are exactly the rows a subprocess makes hardest to set up.
"""

import os
import sys

import pytest


DEFAULT_SOURCE = "https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt"


class Handover(Exception):
    """Raised in place of the `execve` that would replace the test process."""


@pytest.fixture
def handover(monkeypatch, tmp_path):
    """Call `main()` and return the environment it was about to hand a subcommand.

    The working directory and `HOME` are moved into the temporary tree first, because `main()`
    searches for a configuration file when nothing names one, and the developer's own would make
    every assertion below depend on the machine it ran on.
    """
    from freeports_validate import cli

    home = tmp_path / "home"
    home.mkdir()
    monkeypatch.chdir(home)
    monkeypatch.setenv("HOME", str(home))
    monkeypatch.setenv("XDG_CONFIG_HOME", str(home / "config"))
    for name in list(os.environ):
        if name.startswith("FREEPORTS_"):
            monkeypatch.delenv(name)

    captured = {}

    def fake_execve(path, argv, env):
        captured.update(path=str(path), argv=list(argv), env=dict(env))
        raise Handover()

    monkeypatch.setattr(os, "execve", fake_execve)

    def run(*args, **environment):
        for name, value in environment.items():
            monkeypatch.setenv(name, str(value))
        monkeypatch.setattr(
            sys, "argv", ["freeports-validate", *[str(a) for a in args]]
        )
        with pytest.raises(Handover):
            cli.main()
        return captured

    run.home = home
    run.cli = cli
    return run


def write_config(directory, text, name="freeports.yaml"):
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / name
    path.write_text(text)
    return path


class TestGlobalOptionExtraction:
    def test_a_valued_option_is_recognised_before_the_subcommand(self, handover):
        handed = handover("--key-id", "DEADBEEF", "check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_KEY_ID"] == "DEADBEEF"
        assert handed["argv"][1:] == []

    def test_a_valued_option_is_recognised_after_the_subcommand_too(self, handover):
        handed = handover("check-grants", "--key-id", "DEADBEEF")
        assert handed["env"]["FREEPORTS_VALIDATE_KEY_ID"] == "DEADBEEF"

    def test_the_subcommand_keeps_its_own_arguments(self, handover):
        handed = handover("--key-id", "DEADBEEF", "check-grants", "someone@example.org")
        assert handed["argv"][1:] == ["someone@example.org"]

    def test_a_valued_option_without_a_value_is_refused(self, handover, capsys):
        monkeypatched_argv = ["freeports-validate", "check-grants", "--key-id"]
        import freeports_validate.cli as cli

        sys.argv = monkeypatched_argv
        with pytest.raises(SystemExit) as exit_code:
            cli.main()
        assert exit_code.value.code == 1
        assert "--key-id" in capsys.readouterr().out


class TestTheSourceOption:
    def test_it_accumulates_rather_than_overwriting(self, handover):
        handed = handover("-s", "a/*.rst", "-s", "b/*.rst", "check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == "a/*.rst\nb/*.rst"

    def test_the_long_and_short_spellings_are_the_same_option(self, handover):
        handed = handover("--source", "a/*.rst", "-s", "b/*.rst", "check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == "a/*.rst\nb/*.rst"

    def test_the_order_given_is_the_order_handed_over(self, handover):
        handed = handover(
            "-s", "c/*.rst", "-s", "a/*.rst", "-s", "b/*.rst", "check-grants"
        )
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"].splitlines() == [
            "c/*.rst",
            "a/*.rst",
            "b/*.rst",
        ]

    def test_one_source_is_still_a_list_of_one_line(self, handover):
        handed = handover("-s", "a/*.rst", "check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == "a/*.rst"


class TestTheDefaultSource:
    def test_nothing_naming_a_source_gives_the_published_documentation(self, handover):
        handed = handover("check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == DEFAULT_SOURCE

    def test_the_default_is_always_exported_so_no_script_has_to_know_it(self, handover):
        handed = handover("check-grants")
        assert "FREEPORTS_VALIDATE_SOURCES" in handed["env"]


class TestTheEnvironmentTier:
    def test_one_variable_carries_exactly_one_source(self, handover):
        handed = handover("check-grants", FREEPORTS_VALIDATE_SOURCE="a/*.rst")
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == "a/*.rst"

    @pytest.mark.parametrize(
        "raw",
        [
            "a/*.rst,b/*.rst",
            "a/*.rst b/*.rst",
            "a/*.rst:b/*.rst",
            "a/*.rst;b/*.rst",
        ],
    )
    def test_the_value_is_never_split_on_any_separator(self, handover, raw):
        """The rule `FREEPORTS_TARGET_LIST` already follows.

        Every candidate separator is a legal character in a URI or a path, so splitting would turn
        one source the user meant into two that do not exist -- and the failure would arrive much
        later, as a methodology that cannot be resolved.
        """
        handed = handover("check-grants", FREEPORTS_VALIDATE_SOURCE=raw)
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == raw

    def test_an_empty_variable_names_nothing_and_the_default_stands(self, handover):
        handed = handover("check-grants", FREEPORTS_VALIDATE_SOURCE="")
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == DEFAULT_SOURCE


class TestTheFileTier:
    def test_the_list_is_read_in_order(self, handover, tmp_path):
        config = write_config(
            tmp_path / "cfg", "validate:\n  sources:\n    - /a/*.rst\n    - /b/*.rst\n"
        )
        handed = handover("--config", str(config), "check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == "/a/*.rst\n/b/*.rst"

    def test_an_empty_list_names_nothing_and_the_default_stands(
        self, handover, tmp_path
    ):
        config = write_config(tmp_path / "cfg", "validate:\n  sources: []\n")
        handed = handover("--config", str(config), "check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == DEFAULT_SOURCE

    def test_a_relative_pattern_is_resolved_against_the_file_that_says_it(
        self, handover, tmp_path
    ):
        """The rule `formats_repo` already follows, and for the same reason.

        A configuration file is *searched for* -- working directory, then the user's, then the
        system's -- so one line in it is read from many different working directories and has to
        name the same pages from all of them. Against the working directory it would not.
        """
        config = write_config(tmp_path / "cfg", "validate:\n  sources: [pages/*.rst]\n")
        handed = handover("--config", str(config), "check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == str(
            tmp_path / "cfg" / "pages" / "*.rst"
        )

    def test_an_absolute_pattern_is_left_alone(self, handover, tmp_path):
        config = write_config(
            tmp_path / "cfg", "validate:\n  sources: [/pages/*.rst]\n"
        )
        handed = handover("--config", str(config), "check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == "/pages/*.rst"

    @pytest.mark.parametrize(
        "uri",
        [
            "https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt",
            "http://example.invalid/*.rst",
            "file:///pages/*.rst",
        ],
    )
    def test_a_uri_is_never_treated_as_a_path(self, handover, tmp_path, uri):
        config = write_config(tmp_path / "cfg", f'validate:\n  sources: ["{uri}"]\n')
        handed = handover("--config", str(config), "check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == uri


class TestTiersDoNotMerge:
    """The strongest tier that names any source provides the whole list.

    Merging would make "which text did this hash come from" unanswerable from any one place: the
    answer would be a set assembled from three files nobody reads together. The user's own case --
    the published documentation *plus* one's own methodologies -- is served by listing both in the
    same tier, which is why the file tier takes a list.
    """

    def test_the_command_line_replaces_the_environment(self, handover):
        handed = handover(
            "-s", "cmdline/*.rst", "check-grants", FREEPORTS_VALIDATE_SOURCE="env/*.rst"
        )
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == "cmdline/*.rst"

    def test_the_command_line_replaces_the_file(self, handover, tmp_path):
        config = write_config(tmp_path / "cfg", "validate:\n  sources: [/file/*.rst]\n")
        handed = handover(
            "-s", "cmdline/*.rst", "--config", str(config), "check-grants"
        )
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == "cmdline/*.rst"

    def test_the_environment_replaces_the_file(self, handover, tmp_path):
        config = write_config(tmp_path / "cfg", "validate:\n  sources: [/file/*.rst]\n")
        handed = handover(
            "--config",
            str(config),
            "check-grants",
            FREEPORTS_VALIDATE_SOURCE="env/*.rst",
        )
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == "env/*.rst"

    def test_the_file_replaces_the_default(self, handover, tmp_path):
        config = write_config(tmp_path / "cfg", "validate:\n  sources: [/file/*.rst]\n")
        handed = handover("--config", str(config), "check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_SOURCES"] == "/file/*.rst"


class TestOffline:
    def test_it_is_absent_unless_something_asks_for_it(self, handover):
        handed = handover("check-grants")
        assert "FREEPORTS_VALIDATE_OFFLINE" not in handed["env"]

    def test_the_flag_takes_no_value_and_leaves_the_subcommand_s_arguments_alone(
        self, handover
    ):
        handed = handover("--offline", "check-grants", "someone@example.org")
        assert handed["env"]["FREEPORTS_VALIDATE_OFFLINE"] == "1"
        assert handed["argv"][1:] == ["someone@example.org"]

    def test_the_environment_tier_asks_for_it(self, handover):
        handed = handover("check-grants", FREEPORTS_VALIDATE_OFFLINE="1")
        assert handed["env"]["FREEPORTS_VALIDATE_OFFLINE"] == "1"

    def test_the_environment_tier_can_also_say_no(self, handover):
        handed = handover("check-grants", FREEPORTS_VALIDATE_OFFLINE="0")
        assert "FREEPORTS_VALIDATE_OFFLINE" not in handed["env"]

    def test_the_file_tier_asks_for_it(self, handover, tmp_path):
        config = write_config(tmp_path / "cfg", "validate:\n  offline: true\n")
        handed = handover("--config", str(config), "check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_OFFLINE"] == "1"

    def test_the_flag_overrides_a_file_that_says_no(self, handover, tmp_path):
        config = write_config(tmp_path / "cfg", "validate:\n  offline: false\n")
        handed = handover("--offline", "--config", str(config), "check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_OFFLINE"] == "1"

    def test_the_negative_flag_overrides_a_file_that_says_yes(self, handover, tmp_path):
        config = write_config(tmp_path / "cfg", "validate:\n  offline: true\n")
        handed = handover("--no-offline", "--config", str(config), "check-grants")
        assert "FREEPORTS_VALIDATE_OFFLINE" not in handed["env"]

    def test_a_no_from_the_environment_is_not_silence(self, handover, tmp_path):
        """A tier that says no must not be read as a tier that said nothing.

        `FREEPORTS_VALIDATE_OFFLINE=0` has to beat a configuration file that says yes, which is
        what the module's own comment about this setting has always claimed and what the resolution
        now actually does.
        """
        config = write_config(tmp_path / "cfg", "validate:\n  offline: true\n")
        handed = handover(
            "--config", str(config), "check-grants", FREEPORTS_VALIDATE_OFFLINE="0"
        )
        assert "FREEPORTS_VALIDATE_OFFLINE" not in handed["env"]


class TestDeep:
    """The same three tiers as `offline`, and the opposite default.

    Following what a page pins is **on** unless something says otherwise. A page's `.. sha256:`
    lines are part of the page, so its own hash already commits to them; checking them is what
    answers the transitive question, whether the things the page relies on still say what its author
    read. That is the question somebody deciding what a grant is worth is actually asking, and it
    should not be the one they have to know to ask for. It costs a fetch per pinned resource, so
    there is a way to say no -- and because it is on by default, saying no is what the command line
    has to be able to express.
    """

    def test_it_is_on_when_nothing_says_otherwise(self, handover):
        handed = handover("check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_DEEP"] == "1"

    def test_the_negative_flag_turns_it_off(self, handover):
        handed = handover("--no-deep", "check-grants")
        assert "FREEPORTS_VALIDATE_DEEP" not in handed["env"]

    def test_shallow_is_the_same_thing_said_the_other_way(self, handover):
        handed = handover("--shallow", "check-grants")
        assert "FREEPORTS_VALIDATE_DEEP" not in handed["env"]

    def test_the_positive_flag_still_works_and_takes_no_value(self, handover):
        handed = handover("--deep", "check-methodology", "basic check")
        assert handed["env"]["FREEPORTS_VALIDATE_DEEP"] == "1"
        assert handed["argv"][1:] == ["basic check"]

    def test_the_negative_flag_leaves_the_subcommand_s_arguments_alone(self, handover):
        handed = handover("--no-deep", "check-methodology", "basic check")
        assert handed["argv"][1:] == ["basic check"]

    def test_the_environment_tier_can_turn_it_off(self, handover):
        handed = handover("check-grants", FREEPORTS_VALIDATE_DEEP="0")
        assert "FREEPORTS_VALIDATE_DEEP" not in handed["env"]

    def test_the_environment_tier_can_turn_it_on_again(self, handover):
        handed = handover("check-grants", FREEPORTS_VALIDATE_DEEP="1")
        assert handed["env"]["FREEPORTS_VALIDATE_DEEP"] == "1"

    def test_the_file_tier_can_turn_it_off(self, handover, tmp_path):
        config = write_config(tmp_path / "cfg", "validate:\n  deep: false\n")
        handed = handover("--config", str(config), "check-grants")
        assert "FREEPORTS_VALIDATE_DEEP" not in handed["env"]

    def test_the_flag_overrides_a_file_that_says_no(self, handover, tmp_path):
        config = write_config(tmp_path / "cfg", "validate:\n  deep: false\n")
        handed = handover("--deep", "--config", str(config), "check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_DEEP"] == "1"

    def test_the_negative_flag_overrides_a_file_that_says_yes(self, handover, tmp_path):
        config = write_config(tmp_path / "cfg", "validate:\n  deep: true\n")
        handed = handover("--no-deep", "--config", str(config), "check-grants")
        assert "FREEPORTS_VALIDATE_DEEP" not in handed["env"]

    def test_a_no_from_the_environment_is_not_silence(self, handover, tmp_path):
        """The bug this guards against: `_first` treats ``False`` as "nothing was said".

        A tier that says **no** has said something, and the tier below it must not get to answer
        instead. Without that distinction `FREEPORTS_VALIDATE_DEEP=0` would be overridden by a
        configuration file that says yes, which is the opposite of what the tiers mean.
        """
        config = write_config(tmp_path / "cfg", "validate:\n  deep: true\n")
        handed = handover(
            "--config", str(config), "check-grants", FREEPORTS_VALIDATE_DEEP="0"
        )
        assert "FREEPORTS_VALIDATE_DEEP" not in handed["env"]

    def test_it_is_independent_of_offline(self, handover, tmp_path):
        """Deliberately combinable: `--deep --offline` is "check everything, from what I already
        have", which is what an audit on a train actually wants."""
        handed = handover("--deep", "--offline", "check-grants")
        assert handed["env"]["FREEPORTS_VALIDATE_DEEP"] == "1"
        assert handed["env"]["FREEPORTS_VALIDATE_OFFLINE"] == "1"


class TestTheRetiredDocsVariable:
    def test_no_subcommand_is_told_where_the_shipped_pages_would_be(self, handover):
        """`FREEPORTS_VALIDATE_DOCS` named the copy that shipped inside the package.

        There is nothing left for it to name, and leaving it exported would be an invitation to
        write a script that reads it and works only on an installation that still has the old
        directory lying around.
        """
        handed = handover("check-grants")
        assert "FREEPORTS_VALIDATE_DOCS" not in handed["env"]


class TestUsageAndExits:
    def test_no_arguments_prints_usage_and_exits_one(self, capsys):
        from freeports_validate import cli

        sys.argv = ["freeports-validate"]
        with pytest.raises(SystemExit) as exit_code:
            cli.main()
        assert exit_code.value.code == 1
        assert "Usage: freeports-validate" in capsys.readouterr().out

    def test_help_exits_zero(self, capsys):
        from freeports_validate import cli

        sys.argv = ["freeports-validate", "--help"]
        with pytest.raises(SystemExit) as exit_code:
            cli.main()
        assert exit_code.value.code == 0
        assert "Subcommands:" in capsys.readouterr().out

    def test_the_usage_documents_every_subcommand_that_exists(self, capsys):
        from freeports_validate import cli

        sys.argv = ["freeports-validate", "--help"]
        with pytest.raises(SystemExit):
            cli.main()
        printed = capsys.readouterr().out
        for subcommand in cli.SUBCOMMANDS:
            assert subcommand in printed

    def test_every_documented_subcommand_has_a_script(self):
        from pathlib import Path

        from freeports_validate import cli

        bin_dir = Path(cli.__file__).parent / "bin"
        for subcommand in cli.SUBCOMMANDS:
            assert (bin_dir / subcommand).is_file(), subcommand

    def test_an_unknown_subcommand_is_refused_and_the_known_ones_listed(self, capsys):
        from freeports_validate import cli

        sys.argv = ["freeports-validate", "no-such-thing"]
        with pytest.raises(SystemExit) as exit_code:
            cli.main()
        assert exit_code.value.code == 1
        printed = capsys.readouterr().out
        assert "no-such-thing" in printed
        assert "check-grants" in printed
