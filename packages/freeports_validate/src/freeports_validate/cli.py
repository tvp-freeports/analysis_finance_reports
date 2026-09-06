"""The ``freeports-validate`` command line.

The subcommands are shell scripts. This module is the one place that decides *what they are told*:
it resolves the settings the same way the other two commands do -- command line, then environment,
then configuration file -- and hands the answers to the script it hands over to, as environment
variables. The scripts themselves know nothing about configuration files, and there is no second
implementation of the search for one.

Settings come from three places, named consistently with the rest of the project::

    validate.key_id             in the configuration file
    FREEPORTS_VALIDATE_KEY_ID   in the environment
    --key-id / -k               on the command line

and the formats repository, which is shared with `freeports` and `freeports-dev`, keeps its shared
name in all three: `formats_repo`, ``FREEPORTS_FORMATS_REPO_PATH``, ``--repo``/``-r``.

Reading the configuration file needs the engine, which this package does not require: verifying
somebody else's grants should not mean installing a PDF extractor. So the import is optional, and
without it the command still works from the command line and the environment -- it only loses the
file. Install ``freeports-validate[config]`` to have it.
"""

import os
import sys
from pathlib import Path


SUBCOMMANDS = [
    "grant",
    "ungrant",
    "who-grants",
    "granted-by",
    "granted-with",
    "sign-document",
    "create-document",
    "check-grants",
    "update",
]

#: The global options, each taking one value, recognised anywhere on the command line and removed
#: before the subcommand script sees its own arguments.
GLOBAL_OPTIONS = {
    "--repo": "repo",
    "-r": "repo",
    "--formats-directory": "repo",
    "-F": "repo",
    "--key-id": "key_id",
    "-k": "key_id",
    "--config": "config",
}


def _extract_global_options(args):
    """Pull the global options out of ``args``, returning them and what is left.

    Recognised wherever they appear, not only before the subcommand, because that is how the command
    has always accepted ``--repo`` and because none of the subcommand scripts takes an option by any
    of these names.
    """
    values = {}
    rest = []
    i = 0
    while i < len(args):
        name = GLOBAL_OPTIONS.get(args[i])
        if name is not None:
            if i + 1 >= len(args):
                print(f"Error: {args[i]} needs a value")
                sys.exit(1)
            values[name] = args[i + 1]
            i += 2
            continue
        rest.append(args[i])
        i += 1
    return values, rest


def _file_config(config_arg):
    """The configuration file, or ``None`` when there is none or the engine is not installed."""
    try:
        from freeports.cli import FreeportsFileConfig
    except ImportError:
        if config_arg:
            print(
                "Error: --config needs the freeports engine, which is not installed. "
                "Install freeports-validate[config], or give the setting on the command line."
            )
            sys.exit(1)
        return None

    if config_arg:
        return FreeportsFileConfig(str(Path(config_arg).expanduser()))
    path = os.environ.get("FREEPORTS_CONFIG_FILE") or FreeportsFileConfig.find_config()
    if not path:
        return None
    try:
        return FreeportsFileConfig(str(path))
    except Exception as exc:  # noqa: BLE001 -- a broken file must not stop an unrelated subcommand
        print(
            f"Warning: ignoring unusable configuration file {path}: {exc}",
            file=sys.stderr,
        )
        return None


def _first(*candidates):
    for candidate in candidates:
        if candidate:
            return candidate
    return None


def _usage():
    print("Usage: freeports-validate [options] <subcommand> [arguments]")
    print()
    print("Subcommands:")
    for sc in SUBCOMMANDS:
        print(f"  {sc}")
    print()
    print("Options:")
    print(
        "  --repo, -r, --formats-directory, -F PATH   Formats repository\n"
        "        [default: $FREEPORTS_FORMATS_REPO_PATH, then `formats_repo` in the configuration\n"
        "        file, then the enclosing Git repository, then the working directory]"
    )
    print(
        "  --key-id, -k ID                            GPG key the grants are signed with\n"
        "        Needed only by the subcommands that act in your name -- create-document, grant,\n"
        "        ungrant, update, sign-document -- and by check-grants when asked about your own\n"
        "        document. Reading somebody else's grants needs no key.\n"
        "        [default: $FREEPORTS_VALIDATE_KEY_ID, then `validate.key_id` in the configuration\n"
        "        file]"
    )
    print(
        "  --config PATH                              Configuration file to read\n"
        "        [default: $FREEPORTS_CONFIG_FILE, then the file the engine would find]"
    )


def main():
    pkg_dir = Path(__file__).parent
    bin_dir = pkg_dir / "bin"
    lib_dir = pkg_dir / "lib"
    docs_dir = pkg_dir / "docs"

    options, args = _extract_global_options(list(sys.argv[1:]))

    if not args or args[0] in ("-h", "--help"):
        _usage()
        sys.exit(0 if args else 1)

    subcommand = args[0]
    script = bin_dir / subcommand
    if not script.exists():
        print(f"Unknown subcommand: {subcommand}")
        print(f"Available: {', '.join(SUBCOMMANDS)}")
        sys.exit(1)

    file_config = _file_config(options.get("config"))

    def from_file(attribute):
        return (
            getattr(file_config, attribute, None) if file_config is not None else None
        )

    env = os.environ.copy()

    # Left unset when nothing names it, so that `lib/utils` can fall back to the enclosing Git
    # repository. That fallback is this command's own default and worth keeping: a validation
    # document lives at the repository root, and grants are usually issued from deep inside the tree.
    repo = _first(
        options.get("repo"),
        os.environ.get("FREEPORTS_FORMATS_REPO_PATH"),
        from_file("FORMATS_REPO_PATH"),
    )
    if repo:
        env["FREEPORTS_FORMATS_REPO_PATH"] = os.path.abspath(
            str(Path(repo).expanduser())
        )

    key_id = _first(
        options.get("key_id"),
        os.environ.get("FREEPORTS_VALIDATE_KEY_ID"),
        from_file("VALIDATE_KEY_ID"),
    )
    if key_id:
        env["FREEPORTS_VALIDATE_KEY_ID"] = key_id

    env["FREEPORTS_VALIDATE_LIB"] = str(lib_dir)
    env["FREEPORTS_VALIDATE_DOCS"] = str(docs_dir)

    os.execve(str(script), [str(script)] + args[1:], env)
