"""Where ``freeports-dev`` gets its settings, and in what order.

The order is the engine's own, minus the tiers that make no sense here: **command line, then
environment, then configuration file, then the tool's default**. A setting given twice is resolved
per setting, never per source, so naming the repository on the command line does not discard the
target lists the file declares.

Nothing in this module knows where a configuration file lives, what it may be called or which of the
working-directory, user and system tiers wins. That is the engine's answer and there is exactly one
of it, reached through :class:`freeports.cli.FreeportsFileConfig`. Duplicating it here is how two
programs started from the same directory end up reading different files.

The names line up on purpose, so that one setting has one name written three ways::

    dev.page_type            in the configuration file
    FREEPORTS_DEV_PAGE_TYPE  in the environment
    --page-type              on the command line

Settings shared with the engine -- the formats repository, the input database -- keep their engine
names and live at the **top level** of the configuration file, which is what lets a format author
write the repository path once and have all three commands find it.
"""

import os
import warnings
from pathlib import Path

#: What ``--page-type`` means when nobody says otherwise.
DEFAULT_PAGE_TYPE = "investments"

#: The single list ``freeports-dev setup-input-db`` writes, and what a format repository's tests
#: search for unless the repository says otherwise.
DEFAULT_TARGET_LISTS = ["TEST"]


def _env(name):
    value = os.environ.get(name)
    return value if value else None


def _env_bool(name):
    value = _env(name)
    if value is None:
        return None
    lowered = value.lower()
    if lowered in ("true", "yes", "1", "y", "t"):
        return True
    if lowered in ("false", "no", "0", "n", "f"):
        return False
    raise ValueError(f"invalid value for {name}: {value!r}")


def _first(*candidates):
    """The first candidate that is not ``None``. Absent is not the same as false or empty."""
    for candidate in candidates:
        if candidate is not None:
            return candidate
    return None


class DevConfig:
    """The settings of one ``freeports-dev`` invocation, already resolved.

    Built from the parsed arguments, so every subcommand reads its settings the same way instead of
    each one inventing its own fallbacks.
    """

    def __init__(self, args=None):
        self._args = args
        self._file = self._load_file(getattr(args, "config", None))

    # -- the configuration file -------------------------------------------------------------

    @staticmethod
    def _load_file(config_arg):
        """The file the engine would read, or ``None`` if there is none.

        A file named explicitly is loaded outright, so a mistake in its path is reported. One merely
        discovered by searching is not allowed to break the command: its absence is the normal case,
        and a stale file in a home directory should not make an unrelated run fail.
        """
        from freeports.cli import FreeportsFileConfig

        if config_arg:
            return FreeportsFileConfig(str(Path(config_arg).expanduser()))
        path = _env("FREEPORTS_CONFIG_FILE") or FreeportsFileConfig.find_config()
        if not path:
            return None
        try:
            return FreeportsFileConfig(str(path))
        except Exception as exc:  # noqa: BLE001 -- a broken file must not stop an unrelated command
            warnings.warn(f"ignoring unusable configuration file {path}: {exc}")
            return None

    def _from_file(self, attribute):
        return getattr(self._file, attribute, None) if self._file is not None else None

    def _arg(self, name):
        return getattr(self._args, name, None) if self._args is not None else None

    # -- settings shared with the engine ----------------------------------------------------

    @property
    def formats_repo(self):
        """The formats repository. Defaults to the working directory, as every subcommand did."""
        resolved = _first(
            self._arg("repo"),
            _env("FREEPORTS_FORMATS_REPO_PATH"),
            self._from_file("FORMATS_REPO_PATH"),
        )
        return Path(resolved).expanduser().resolve() if resolved else Path.cwd()

    @property
    def input_db_override(self):
        """An input database named outright, or ``None`` to let the search in :mod:`input_db` run.

        Deliberately not resolved to a default here: the search that follows prefers a format
        repository's **own** ``tests/input_db`` over anything the configuration file says, so that
        running a repository's tests does not silently pick up whatever real database happens to be
        configured on the machine. Only an explicit flag or variable overrides that.
        """
        return _first(self._arg("db_directory"), _env("FREEPORTS_INPUT_DB_PATH"))

    @property
    def input_db_from_file(self):
        """The `db_path` of the configuration file, consulted only after the repository's own."""
        return self._from_file("INPUT_DB_PATH")

    # -- settings of this tool alone --------------------------------------------------------

    @property
    def target_lists(self):
        """The lists the tests search for.

        The environment variable holds **one** list, the whole raw value, never split -- the same
        rule as the engine's ``FREEPORTS_TARGET_LIST``, so that a list name containing a comma or a
        space means the same thing to both commands.
        """
        env = _env("FREEPORTS_DEV_TARGET_LIST")
        return _first(
            self._arg("target_list"),
            [env] if env is not None else None,
            self._from_file("DEV_TARGET_LISTS"),
            DEFAULT_TARGET_LISTS,
        )

    @property
    def noconfirm(self):
        """Skip the ``make-tests`` prompts.

        An absent flag is *unset*, not false: ``--noconfirm`` can only ever turn the setting on, so
        that a command line which does not mention it leaves alone a file or environment that did.
        """
        return _first(
            True if self._arg("noconfirm") else None,
            _env_bool("FREEPORTS_DEV_NOCONFIRM"),
            self._from_file("DEV_NOCONFIRM"),
            False,
        )

    @property
    def page_type(self):
        """The page type, for the subcommands that take one."""
        return _first(
            self._arg("page_type"),
            _env("FREEPORTS_DEV_PAGE_TYPE"),
            self._from_file("DEV_PAGE_TYPE"),
            DEFAULT_PAGE_TYPE,
        )


#: The configuration the command line resolved, for the parts of this package that pytest reaches
#: without going through :func:`freeports_dev.cli.main` -- the plugin above all.
_ACTIVE = None


def set_active(config):
    """Record the configuration this process resolved from its command line.

    The pytest plugin runs in the same process as ``freeports-dev test`` but is reached *through*
    pytest, which knows nothing of our parsed arguments. Rather than smuggle the answers back out
    through the environment -- which cannot even express a list of target lists -- the command line
    leaves the resolved object here and the plugin picks it up.
    """
    global _ACTIVE
    _ACTIVE = config


def active():
    """The recorded configuration, or a freshly resolved one when pytest was run directly."""
    return _ACTIVE if _ACTIVE is not None else DevConfig()
