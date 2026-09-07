"""The ``freeports-validate`` command line.

The subcommands are shell scripts. This module is the one place that decides *what they are told*:
it resolves the settings the same way the other two commands do -- command line, then environment,
then configuration file -- and hands the answers to the script it hands over to, as environment
variables. The scripts themselves know nothing about configuration files, and there is no second
implementation of the search for one.

Every setting comes from three places, named consistently with the rest of the project::

    validate.key_id             FREEPORTS_VALIDATE_KEY_ID   --key-id / -k
    validate.sources            FREEPORTS_VALIDATE_SOURCE    --source / -s
    validate.offline            FREEPORTS_VALIDATE_OFFLINE   --offline / --no-offline
    validate.deep               FREEPORTS_VALIDATE_DEEP      --deep / --no-deep  (on by default)

and the formats repository, which is shared with `freeports` and `freeports-dev`, keeps its shared
name in all three: `formats_repo`, ``FREEPORTS_FORMATS_REPO_PATH``, ``--repo``/``-r``.

**Sources are a list, and the tiers do not merge.** The methodology pages no longer ship with this
command: a grant is made under a text resolved from a *source*, which is a pattern with one ``*`` in
it standing for the methodology's name. Several sources are allowed, and their order is priority --
a name is resolved from the first that offers it. The strongest tier that names any source provides
the whole list; a command line with ``-s`` ignores the file's list entirely.

That is the engine's model for every other setting, and merging would make the question "which text
did this hash come from" unanswerable from any one place -- the answer would be a set assembled from
three files nobody reads together. The case that wants two sources at once -- the published
documentation *plus* one's own methodologies -- is served by listing both in the same tier, which is
why the file tier takes a list.

The environment tier carries **one** source, and its value is never split. Every separator one might
pick -- comma, colon, space, semicolon -- is a legal character in a URI or a path, so splitting would
turn one source the user meant into two that do not exist. This is the same rule
``FREEPORTS_TARGET_LIST`` follows, for the same reason.

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
    "sources",
    "check-methodology",
    "refresh-links",
    "collect",
    "report",
]

#: One line each, for the listing `--help` prints. Only where the name does not already say it:
#: `grant` and `check-grants` do, and a gloss on those would be noise.
SUBCOMMAND_SUMMARIES = {
    "sources": "   Where methodology pages resolve from, and what each name resolves to",
    "check-methodology": " One page in detail: where it came from, and what it pins",
    "refresh-links": "     Re-pin what a local methodology page cites (changes its hash)",
    "collect": "           The coverage model as JSON: who vouched for what, and whether it holds",
    "report": "            The same model rendered -- a page, badges, or one of seven tables",
}

#: Where methodologies come from when no tier names anything. The published documentation, whose
#: reStructuredText sources Sphinx serves under ``_sources/`` with a ``.txt`` suffix appended -- so
#: the extension of a source pattern pointing there is ``.rst.txt`` and not ``.rst``.
DEFAULT_SOURCE = "https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt"

#: Whether a check also follows what a methodology page pins, when no tier says otherwise.
#:
#: On. A page's `.. sha256:` comments are lines of the page, so its own hash already commits to
#: them — but only checking them answers the transitive question, whether the things the page relies
#: on still say what its author read. That is the question somebody deciding what a grant is worth
#: is actually asking, and it should not be the one they have to know to ask for. It costs one fetch
#: per pinned resource; `--no-deep` turns it off, as does `validate.deep: false`.
DEFAULT_DEEP = True

#: The global options taking one value, recognised anywhere on the command line and removed before
#: the subcommand script sees its own arguments. A repeated one is the last one: these name a single
#: thing, and two answers to "which key" is a mistake rather than a list.
GLOBAL_OPTIONS = {
    "--repo": "repo",
    "-r": "repo",
    "--formats-directory": "repo",
    "-F": "repo",
    "--key-id": "key_id",
    "-k": "key_id",
    "--config": "config",
}

#: The global options that accumulate. ``-s`` is repeatable because the setting it feeds *is* a
#: list, and the alternative -- one option holding several sources separated by something -- has no
#: separator available that cannot appear inside a URI.
GLOBAL_LIST_OPTIONS = {
    "--source": "sources",
    "-s": "sources",
}

#: The global options that take no value at all, and the value each one asserts.
#:
#: A flag comes in pairs where the setting it feeds is on by default, because a flag can otherwise
#: only ever switch a setting *on*: with `--deep` alone there would be no way to say no on the
#: command line to something a configuration file had said yes to, and the strongest tier would be
#: the one unable to express half the answers.
GLOBAL_FLAGS = {
    "--offline": ("offline", True),
    "--no-offline": ("offline", False),
    "--deep": ("deep", True),
    "--no-deep": ("deep", False),
    "--shallow": ("deep", False),
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
        flag = GLOBAL_FLAGS.get(args[i])
        if flag is not None:
            name, asserted = flag
            values[name] = asserted
            i += 1
            continue
        listed = GLOBAL_LIST_OPTIONS.get(args[i])
        name = GLOBAL_OPTIONS.get(args[i]) if listed is None else None
        if listed is not None or name is not None:
            if i + 1 >= len(args):
                print(f"Error: {args[i]} needs a value")
                sys.exit(1)
            if listed is not None:
                values.setdefault(listed, []).append(args[i + 1])
            else:
                values[name] = args[i + 1]
            i += 2
            continue
        rest.append(args[i])
        i += 1
    return values, rest


def _file_config(config_arg):
    """The configuration file and its path, or ``(None, None)``.

    The path is returned alongside the settings because a path *inside* the file is meaningless
    without knowing where the file is -- see :func:`_from_config_dir`.
    """
    try:
        from freeports.cli import FreeportsFileConfig
    except ImportError:
        if config_arg:
            print(
                "Error: --config needs the freeports engine, which is not installed. "
                "Install freeports-validate[config], or give the setting on the command line."
            )
            sys.exit(1)
        return None, None

    if config_arg:
        path = Path(config_arg).expanduser()
        return FreeportsFileConfig(str(path)), path
    path = os.environ.get("FREEPORTS_CONFIG_FILE") or FreeportsFileConfig.find_config()
    if not path:
        return None, None
    try:
        return FreeportsFileConfig(str(path)), Path(path)
    except Exception as exc:  # noqa: BLE001 -- a broken file must not stop an unrelated subcommand
        print(
            f"Warning: ignoring unusable configuration file {path}: {exc}",
            file=sys.stderr,
        )
        return None, None


def _from_config_dir(value, config_path):
    """A path the configuration file gave, made absolute against the file's own directory.

    Each tier is resolved against the place it was written, and the three places differ. A path
    typed as ``--repo`` or exported into the environment is relative to the shell you are standing
    in: that is the only thing it can mean, and it is what you meant. A path in a configuration file
    is not. The file is *searched for* -- the working directory, then the user's, then the system's
    -- so one line is read from many different working directories and has to name the same
    repository from all of them. Relative to the file that says it, it does.

    Resolving it against the working directory instead is how ``formats_repo: my-formats``, in a
    file sitting in the repository it names, became ``my-formats/my-formats`` as soon as anyone ran
    a subcommand from inside that repository -- which is where `freeports-validate` is meant to be
    run from, since grants are issued from wherever in the tree the granted file happens to be.
    """
    if not value:
        return None
    path = Path(str(value)).expanduser()
    if path.is_absolute() or config_path is None:
        return str(path)
    return str(Path(config_path).expanduser().resolve().parent / path)


def _source_from_config_dir(source, config_path):
    """A source pattern the configuration file gave, made absolute if it is a bare relative path.

    The same rule as :func:`_from_config_dir`, and for the same reason: the configuration file is
    *searched for*, so one line in it is read from many different working directories and has to
    name the same pages from all of them.

    A URI is never a path, and is left exactly as written -- including ``file://``, which is already
    absolute by construction. The test for one is a scheme separator before the first slash, which
    is enough here: a relative path containing ``://`` is not something anybody writes by accident,
    and a source that is a URI is the overwhelmingly common case.
    """
    text = str(source)
    if _looks_like_a_uri(text):
        return text
    path = Path(text).expanduser()
    if path.is_absolute() or config_path is None:
        return str(path)
    return str(Path(config_path).expanduser().resolve().parent / path)


def _looks_like_a_uri(text):
    """True for ``scheme://rest``, which is the only URI shape a source may take.

    Deliberately narrow. What this has to separate is a URI from a *path*, and a path may contain a
    colon anywhere except in a leading, alphanumeric, slashless scheme -- so the scheme is required
    to be exactly that, and anything else is read as a path and resolved as one.
    """
    scheme, separator, _rest = text.partition("://")
    return bool(separator) and scheme != "" and scheme.isascii() and scheme.isalnum()


def _first(*candidates):
    for candidate in candidates:
        if candidate:
            return candidate
    return None


def _first_defined(*candidates):
    """The first tier that said anything at all, counting ``False`` as having said something.

    :func:`_first` cannot be used for a boolean, and the difference is not pedantic. It returns the
    first *truthy* candidate, so a tier that says **no** looks exactly like a tier that says nothing
    and the next one down answers instead — which is how `FREEPORTS_VALIDATE_DEEP=0` would end up
    overridden by a configuration file that says yes. For a boolean the three states are yes, no and
    silent, and only silence may fall through.
    """
    for candidate in candidates:
        if candidate is not None:
            return candidate
    return None


def _as_bool(value):
    """A boolean setting as the environment can carry it, or ``None`` when nothing said anything.

    ``0``, ``false``, ``no`` and the empty string are all a deliberate *no*, and a no from the
    environment must not read as silence -- otherwise `FREEPORTS_VALIDATE_OFFLINE=0` would fall
    through to a configuration file that says yes, which is the opposite of what the tiers mean.
    """
    if value is None:
        return None
    return value.strip().lower() not in ("", "0", "false", "no", "off")


def _usage():
    print("Usage: freeports-validate [options] <subcommand> [arguments]")
    print()
    print("Subcommands:")
    for sc in SUBCOMMANDS:
        print(f"  {sc}{SUBCOMMAND_SUMMARIES.get(sc, '')}")
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
        "  --source, -s URI                           Where methodology pages are resolved from\n"
        "        A pattern with exactly one `*`, standing for the methodology's name: for instance\n"
        "        `general_methodology` or `methodologies/basic_check`. Repeatable, and the order is\n"
        "        priority -- a name comes from the first source that offers it. `freeports-validate\n"
        "        sources` shows what each one resolves.\n"
        "        The tiers do not merge: giving -s here ignores the list in the configuration file.\n"
        "        [default: $FREEPORTS_VALIDATE_SOURCE (one source, never split), then\n"
        "        `validate.sources` in the configuration file, then\n"
        f"        {DEFAULT_SOURCE}]"
    )
    print(
        "  --offline / --no-offline                   Never fetch a methodology page\n"
        "        Answer from what has already been cached, and say so on the line where a check\n"
        "        depended on it. A methodology with nothing cached is reported as unreachable\n"
        "        rather than as matching or mismatched.\n"
        "        [default: $FREEPORTS_VALIDATE_OFFLINE, then `validate.offline` in the\n"
        "        configuration file, then off]"
    )
    print(
        "  --no-deep, --shallow                       Do not follow what a methodology page pins\n"
        "        A page may pin the links and images it cites, by hash, in `.. sha256:` comments.\n"
        "        Those lines are part of the page, so its own hash already commits to them;\n"
        "        following them answers the transitive question, whether the things it cites still\n"
        "        say what its author read. That is ON by default, because it is the question you\n"
        "        are really asking. It costs one fetch per pinned resource, so turn it off where\n"
        "        that matters. Understood by check-methodology and check-grants.\n"
        "        [default: $FREEPORTS_VALIDATE_DEEP, then `validate.deep` in the configuration\n"
        "        file, then ON]"
    )
    print(
        "  --config PATH                              Configuration file to read\n"
        "        [default: $FREEPORTS_CONFIG_FILE, then the file the engine would find]"
    )


def main():
    pkg_dir = Path(__file__).parent
    bin_dir = pkg_dir / "bin"
    lib_dir = pkg_dir / "lib"

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

    file_config, file_config_path = _file_config(options.get("config"))

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
        _from_config_dir(from_file("FORMATS_REPO_PATH"), file_config_path),
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

    # The tiers do not merge: the strongest one that names any source provides the whole list. An
    # empty list is a tier that names nothing -- `sources: []` in a file is a decision to say
    # nothing, not a decision to have no methodologies at all -- so it falls through like an absent
    # key rather than leaving the resolver with nowhere to look.
    file_sources = from_file("VALIDATE_SOURCES")
    env_source = os.environ.get("FREEPORTS_VALIDATE_SOURCE")
    sources = (
        options.get("sources")
        or ([env_source] if env_source else None)
        or [
            _source_from_config_dir(source, file_config_path)
            for source in (file_sources or [])
        ]
        or [DEFAULT_SOURCE]
    )
    # Newline-separated, which is the one separator a URI and a path cannot contain, so the scripts
    # can split the list back apart with no escaping and no ambiguity.
    env["FREEPORTS_VALIDATE_SOURCES"] = "\n".join(sources)

    # Exported only when it is on. A script asks `[ -n "$..." ]`, so an unset variable and an
    # explicit `0` have to be the same thing, and the surest way to make them the same thing is for
    # the variable not to be there.
    #
    # `_first_defined` and not `_first`: a tier that says *no* has said something, and must not be
    # skipped over as though it had been silent.
    offline = _first_defined(
        options.get("offline"),
        _as_bool(os.environ.get("FREEPORTS_VALIDATE_OFFLINE")),
        from_file("VALIDATE_OFFLINE"),
    )
    if offline:
        env["FREEPORTS_VALIDATE_OFFLINE"] = "1"
    else:
        env.pop("FREEPORTS_VALIDATE_OFFLINE", None)

    # The same shape as `offline`, and the opposite default. A methodology page may pin the things
    # it cites, and those pins are part of what a grant was made under: checking them is the
    # difference between "this page still hashes the same" and "what this page relies on still says
    # what its author read". Leaving that off by default made the thorough answer the one you had to
    # know to ask for, which is the wrong way round for a tool whose entire subject is how much a
    # claim is worth. `--no-deep` (or `--shallow`) is there for when the cost matters.
    deep = _first_defined(
        options.get("deep"),
        _as_bool(os.environ.get("FREEPORTS_VALIDATE_DEEP")),
        from_file("VALIDATE_DEEP"),
        DEFAULT_DEEP,
    )
    if deep:
        env["FREEPORTS_VALIDATE_DEEP"] = "1"
    else:
        env.pop("FREEPORTS_VALIDATE_DEEP", None)

    env["FREEPORTS_VALIDATE_LIB"] = str(lib_dir)

    os.execve(str(script), [str(script)] + args[1:], env)
