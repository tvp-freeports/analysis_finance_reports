"""Fixtures for the ``freeports-validate`` suite.

The command is a Python entry point that hands over to shell scripts, so almost nothing here can be
tested by importing a function: what has to be exercised is a *process*, with its environment, its
keyring, its repository and its methodology sources. Every fixture below builds one of those four
things, in a temporary directory, and every one of them is disposable.

Three rules the whole suite is built on:

**No test may require the real internet.** The URL branch of the source resolver, and the cache that
sits under it, are exercised against :fixture:`http_source` -- a ``http.server`` on an ephemeral port
serving a directory this suite wrote. A test that genuinely wants the network carries the
``network`` marker and is deselected by default (see ``[tool.pytest.ini_options]`` in
``pyproject.toml``); nothing in continuous integration should ever need to select it.

**The command under test is the source tree, not the installed copy.** ``pip install`` puts a
snapshot in the environment, and a suite that ran against the snapshot would pass over a change that
was never installed. :fixture:`run_validate` therefore starts a fresh interpreter with
``packages/freeports_validate/src`` at the front of ``sys.path``.

**The run is sealed off from the developer's own setup.** ``HOME``, ``GNUPGHOME``,
``XDG_CACHE_HOME`` and ``XDG_CONFIG_HOME`` all point inside the temporary tree, and every
``FREEPORTS_*`` variable inherited from the shell is dropped. The command searches for a
configuration file when nothing names one, and finding the developer's would make the file tier's
tests depend on the machine they run on.
"""

import importlib.util
import os
import pty
import select
import shutil
import subprocess
import sys
import threading
from dataclasses import dataclass, field
from functools import partial
from hashlib import sha256
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest


PACKAGE_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_SRC = PACKAGE_ROOT / "src"
LIB_DIR = PACKAGE_SRC / "freeports_validate" / "lib"

#: Run ``freeports_validate.cli.main`` out of the source tree. ``python -c CODE a b c`` leaves
#: ``sys.argv`` as ``['-c', 'a', 'b', 'c']``, so the arguments appended to this line arrive at
#: ``main()`` exactly where it looks for them, with no argument shuffling in between.
_BOOTSTRAP = (
    "import sys; sys.path.insert(0, {src!r}); "
    "from freeports_validate.cli import main; main()"
).format(src=str(PACKAGE_SRC))

#: The programs the shell scripts call. They are dependencies of the command rather than of this
#: suite, and none of them can be declared in ``pyproject.toml``; a missing one is therefore a
#: skipped suite with a message naming it, not a wall of failures about `command not found`.
REQUIRED_PROGRAMS = (
    "bash",
    "gpg",
    "sha256sum",
    "realpath",
    "curl",
    "yq",
    "jq",
    "check-jsonschema",
    # The shell scripts call the helpers in `lib/` as `python3`, not as whatever interpreter is
    # running this suite: a virtual environment that put `freeports-validate` on PATH need not have
    # put its own `python3` there, and the helpers deliberately use nothing but the standard library
    # so that any `python3` will do.
    "python3",
)

#: Timeout for one invocation of the command. Generous enough for a `gpg` signature on a cold agent,
#: short enough that a subcommand blocked on a prompt fails the test instead of hanging the suite.
RUN_TIMEOUT = 60.0


# ---------------------------------------------------------------------------
# What is expensive, and how it gets marked
# ---------------------------------------------------------------------------

#: Requesting any of these makes a test `slow`, because each one starts the command as a *process*.
#:
#: The marking is derived rather than written on each test, so that it cannot go stale: a test added
#: next year that invokes the command is marked by the act of asking for the fixture that invokes
#: it, and one that stops invoking the command stops being marked. A hand-written `@pytest.mark.slow`
#: on three hundred tests would be three hundred chances to forget.
#:
#: What makes them expensive is not this suite. It is the command's own architecture: an invocation
#: starts a Python interpreter, hands over to `bash`, and each subcommand shells out to `yq` (itself
#: Python plus `jq`) several times, to `check-jsonschema` once per document, and to `gpg` to sign or
#: verify. Tens of process starts per test, and a suite of them takes minutes.
SLOW_FIXTURES = frozenset({"run_validate", "signed_document"})


def pytest_collection_modifyitems(config, items):
    """Mark every test that starts the command as `slow`.

    The commit hook runs the fast tests, and the slow ones are run on request -- `make
    test-tools-slow`, or `pytest -m slow`. A gate nobody keeps enabled gates nothing, and a suite
    that costs minutes is a gate people disable.
    """
    slow = pytest.mark.slow
    for item in items:
        if SLOW_FIXTURES & set(item.fixturenames):
            item.add_marker(slow)


# ---------------------------------------------------------------------------
# Preconditions
# ---------------------------------------------------------------------------


@pytest.fixture(scope="session", autouse=True)
def _external_programs():
    missing = [name for name in REQUIRED_PROGRAMS if shutil.which(name) is None]
    if missing:
        pytest.skip(
            "freeports-validate needs these programs, and they are not on PATH: "
            + ", ".join(missing)
        )


# ---------------------------------------------------------------------------
# A throwaway keyring
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Signer:
    """The identity the suite grants under: a key, and what the command derives from its UID."""

    fingerprint: str
    name: str
    email: str
    gnupghome: Path

    @property
    def document_name(self):
        """The file `lib/utils` computes for this signer -- name, lowercased, spaces underscored."""
        return self.name.replace(" ", "_").lower() + ".yaml"


def _generate_key(gnupghome, name, email):
    params = gnupghome / "params"
    params.write_text(
        "%no-protection\n"
        "Key-Type: eddsa\n"
        "Key-Curve: Ed25519\n"
        "Key-Usage: sign\n"
        f"Name-Real: {name}\n"
        f"Name-Email: {email}\n"
        "Expire-Date: 0\n"
        "%commit\n"
    )
    env = {**os.environ, "GNUPGHOME": str(gnupghome)}
    subprocess.run(
        ["gpg", "--batch", "--quiet", "--gen-key", str(params)],
        env=env,
        check=True,
        capture_output=True,
    )
    listing = subprocess.run(
        ["gpg", "--list-keys", "--with-colons", email],
        env=env,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    for line in listing.splitlines():
        if line.startswith("fpr:"):
            return line.split(":")[9]
    raise AssertionError(f"gpg produced no fingerprint for {email}:\n{listing}")


@pytest.fixture(scope="session")
def gpg_home(tmp_path_factory):
    """A ``GNUPGHOME`` of its own, holding one unprotected signing key.

    Session-scoped because generating a key and starting an agent costs a second or so and nothing
    in the suite modifies the keyring -- documents are signed *with* it, which leaves it untouched.
    The agent it starts is killed at the end, since a stray `gpg-agent` holding a deleted home open
    is exactly the kind of leftover that makes a suite flaky on the second run.
    """
    home = tmp_path_factory.mktemp("gnupg")
    home.chmod(0o700)
    yield home
    subprocess.run(
        ["gpgconf", "--homedir", str(home), "--kill", "all"],
        capture_output=True,
        check=False,
    )


@pytest.fixture(scope="session")
def signer(gpg_home):
    """The suite's own granter."""
    name, email = "Test Granter", "granter@example.invalid"
    return Signer(
        fingerprint=_generate_key(gpg_home, name, email),
        name=name,
        email=email,
        gnupghome=gpg_home,
    )


@pytest.fixture(scope="session")
def other_signer(gpg_home):
    """A second identity in the same keyring, for the checks that are about *whose* grant it is."""
    name, email = "Second Granter", "second@example.invalid"
    return Signer(
        fingerprint=_generate_key(gpg_home, name, email),
        name=name,
        email=email,
        gnupghome=gpg_home,
    )


# ---------------------------------------------------------------------------
# A repository to grant in
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Repo:
    """A skeleton formats repository: enough of one for the command to agree to work in it."""

    root: Path

    @property
    def validation(self):
        return self.root / "validation"

    def write(self, relative, text="placeholder\n"):
        """Create a file at a repository-relative path, parents included, and return its full path."""
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        return path

    def read(self, relative):
        return (self.root / relative).read_text()

    def sha256(self, relative):
        return sha256((self.root / relative).read_bytes()).hexdigest()

    def document(self, signer):
        return self.validation / signer.document_name


@pytest.fixture
def tmp_repo(tmp_path):
    """A repository shaped like a real formats repository, with files worth granting.

    ``metadata/formats.csv`` is what both `freeports-dev` and `create-document` look for to decide a
    directory is a formats repository, so it is not decoration -- without it the command refuses to
    create `validation/`.

    The files under ``tests/formats/`` follow the real layout, which has a *variant* level between
    the format and its outputs (``tests/formats/FOO-EN24/1/out/...``). That level is easy to forget
    when writing a path pattern from memory, and a fixture that flattened it would let a wrong
    pattern pass.
    """
    root = tmp_path / "repo"
    repo = Repo(root=root)
    repo.write(
        "metadata/formats.csv", "Name,Locale,Year,Country,Version\nFOO,EN,24,,\n"
    )
    repo.validation.mkdir(parents=True, exist_ok=True)
    repo.write("tests/formats/FOO-EN24/1/out/funds.csv", "isin,name\nIT0001,Alpha\n")
    repo.write(
        "tests/formats/FOO-EN24/1/out/investments.csv", "isin,value\nIT0001,10\n"
    )
    repo.write("tests/formats/FOO-EN24/1/pages/investments/1.json", '{"page": 1}\n')
    repo.write("content/FOO/EN24.py", "# a format module\n")
    repo.write("README.md", "# A formats repository\n")
    return repo


# ---------------------------------------------------------------------------
# Methodology pages, and the two ways of publishing them
# ---------------------------------------------------------------------------

GENERAL_METHODOLOGY = """\
General methodology
===================

The protocol every grant in this repository is made under.

A grant is a claim by a person about a file, made under a named methodology, and signed.
"""

BASIC_CHECK = """\
Basic check
===========

The lightest of the methodologies: the run completed and a human looked at the result.
"""

GOLDEN_STANDARD = """\
Golden standard
===============

The heaviest of the methodologies: every value was checked against the source document.
"""


def with_supported_paths(page, *entries, heading="Supported paths"):
    """``page``, with a section declaring each ``(pattern, prose)`` -- or ``(pattern,)`` -- entry.

    Written the way a real page writes it: a visible section, and inside it a definition list whose
    terms are inline literals. Nothing here is a directive or a comment, so the page stays ordinary
    reStructuredText that renders as prose a reader can check the tool's behaviour against.
    """
    block = [page.rstrip("\n"), "", heading, "=" * len(heading), ""]
    for entry in entries:
        pattern = entry[0] if isinstance(entry, tuple) else entry
        prose = entry[1] if isinstance(entry, tuple) and len(entry) > 1 else None
        block.append(f"``{pattern}``")
        if prose is not None:
            block.extend(f"    {line}" for line in prose.splitlines())
        block.append("")
    return "\n".join(block) + "\n"


@dataclass
class PageTree:
    """A directory of methodology pages, laid out the way a source pattern expects to find them.

    The layout is the resolver's, not the filesystem's convenience: the general methodology sits at
    the root as ``general_methodology.rst`` and everything else under ``methodologies/``, because
    that is what the ``*`` of a source pattern is substituted with.
    """

    root: Path

    def write(self, name, text):
        """Write the page for a methodology-relative name (``methodologies/basic_check``)."""
        path = self.root / f"{name}.rst"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        return path

    def path(self, name):
        return self.root / f"{name}.rst"

    def sha256(self, name):
        return sha256(self.path(name).read_bytes()).hexdigest()

    @property
    def file_pattern(self):
        return f"file://{self.root}/*.rst"

    @property
    def bare_pattern(self):
        """The same tree named as a plain path -- the spelling a source may also be written in."""
        return f"{self.root}/*.rst"


@pytest.fixture
def methodology_pages(tmp_path):
    """The three pages every test needs present before it writes any of its own."""
    tree = PageTree(root=tmp_path / "methodologies-source")
    tree.write("general_methodology", GENERAL_METHODOLOGY)
    tree.write("methodologies/basic_check", BASIC_CHECK)
    tree.write("methodologies/golden_standard", GOLDEN_STANDARD)
    return tree


@pytest.fixture
def local_source(methodology_pages):
    """:fixture:`methodology_pages` published as a ``file://`` source pattern."""
    return methodology_pages.file_pattern


class _RecordingHandler(SimpleHTTPRequestHandler):
    """A static handler that keeps the list of paths it was asked for.

    The list is what makes a cache test a cache test: "the second run did not hit the network" is
    only observable from the server's side, and asserting on timing instead would be a flaky test
    about a fast disk.
    """

    def __init__(self, *args, recorder=None, **kwargs):
        self._recorder = recorder
        super().__init__(*args, **kwargs)

    def do_GET(self):
        self._recorder.append(self.path)
        super().do_GET()

    def do_HEAD(self):
        self._recorder.append(self.path)
        super().do_HEAD()

    def log_message(self, *args):
        """Silence: the access log would interleave with pytest's own output."""


@dataclass
class HttpSource:
    """A methodology tree served over HTTP on an ephemeral port of the loopback interface."""

    tree: PageTree
    base_url: str
    requested: list = field(default_factory=list)
    _server: object = None

    @property
    def pattern(self):
        return f"{self.base_url}/*.rst"

    def url(self, name):
        return f"{self.base_url}/{name}.rst"

    def stop(self):
        """Take the server down, so that a later fetch fails the way an offline machine fails."""
        if self._server is not None:
            self._server.shutdown()
            self._server.server_close()
            self._server = None


@pytest.fixture
def http_source(methodology_pages):
    """:fixture:`methodology_pages` published over HTTP, with no internet involved.

    Bound to ``127.0.0.1`` on port ``0``: the loopback interface needs no network to exist, and port
    zero means two runs of the suite in parallel cannot collide over a hard-coded number.
    """
    requested = []
    handler = partial(
        _RecordingHandler,
        directory=str(methodology_pages.root),
        recorder=requested,
    )
    server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    host, port = server.server_address[:2]
    source = HttpSource(
        tree=methodology_pages,
        base_url=f"http://{host}:{port}",
        requested=requested,
        _server=server,
    )
    yield source
    source.stop()
    thread.join(timeout=5)


# ---------------------------------------------------------------------------
# Running the command
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Run:
    """What one invocation of the command did."""

    args: list
    returncode: int
    stdout: str
    stderr: str

    @property
    def output(self):
        """Both streams, for the assertions that do not care which one a line came out of.

        Most of what this command prints is meant for a person: successes on stdout, errors and the
        end-of-run explanations on stderr. A test about *what was said* should not have to know
        which, and a test about the split says so by reading `stdout` or `stderr` directly.
        """
        return self.stdout + self.stderr

    def __str__(self):
        return (
            f"$ freeports-validate {' '.join(self.args)}\n"
            f"[exit {self.returncode}]\n{self.output}"
        )


class Sealed:
    """The environment one test runs in, and nothing that runs in it.

    Split out of :class:`Runner` so that the fixture graph says something true about cost. Starting
    the *command* is expensive -- a Python interpreter, then `bash`, then `yq` and
    `check-jsonschema` and `gpg`, several processes deep -- while running a shell library directly
    is not. Both need the same sealed `HOME`, `XDG_*` and `GNUPGHOME`, and when the cheap fixtures
    obtained that by requiring the expensive one, every test that touched a shell function looked,
    to anything reading `item.fixturenames`, exactly like a test that started the command.

    That distinction is what `pytest_collection_modifyitems` marks `slow` on, so it has to be real.
    """

    def __init__(self, home, cache_home, config_home, gnupghome):
        self.home = home
        self.cache_home = cache_home
        self.config_home = config_home
        self.gnupghome = gnupghome

    def environment(self, extra=None):
        env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith("FREEPORTS_")
        }
        env.update(
            {
                "HOME": str(self.home),
                "XDG_CACHE_HOME": str(self.cache_home),
                "XDG_CONFIG_HOME": str(self.config_home),
                "GNUPGHOME": str(self.gnupghome),
                # Diagnoses are matched by their wording, and a localised `gpg` or `sort` would
                # change it under the test rather than under the code.
                "LC_ALL": "C",
                "LANG": "C",
                # `git rev-parse` is `lib/utils`' fallback for the repository root. Under a temporary
                # directory there is normally no checkout to find, but a developer whose /tmp is
                # inside one would get a root nobody chose, and the failure would be theirs alone.
                "GIT_CEILING_DIRECTORIES": str(self.home.parent),
            }
        )
        if extra:
            env.update({k: str(v) for k, v in extra.items()})
        return env


class Runner:
    """Starts the command under test inside a :class:`Sealed` environment."""

    def __init__(self, sealed):
        self.sealed = sealed

    #: Delegated rather than re-derived, so that a test holding a `run_validate` still reaches the
    #: same directories the shell-library fixtures were given.
    @property
    def home(self):
        return self.sealed.home

    @property
    def cache_home(self):
        return self.sealed.cache_home

    def environment(self, extra=None):
        return self.sealed.environment(extra)

    def __call__(
        self,
        *args,
        repo=None,
        key_id=None,
        sources=None,
        env=None,
        cwd=None,
        stdin="",
        tty=False,
        timeout=RUN_TIMEOUT,
    ):
        """Run the command and return a :class:`Run`.

        `repo`, `key_id` and `sources` are conveniences for the three settings nearly every test
        names; they go in through the *environment* tier. A test about the command line passes the
        options in `args` instead, and a test about the file tier writes a file and passes
        ``--config``.
        """
        extra = dict(env or {})
        if repo is not None:
            extra["FREEPORTS_FORMATS_REPO_PATH"] = str(
                repo.root if isinstance(repo, Repo) else repo
            )
        if key_id is not None:
            extra["FREEPORTS_VALIDATE_KEY_ID"] = key_id
        if sources is not None:
            extra["FREEPORTS_VALIDATE_SOURCE"] = sources
        full_env = self.environment(extra)
        argv = [str(a) for a in args]
        command = [sys.executable, "-c", _BOOTSTRAP, *argv]
        directory = str(cwd) if cwd is not None else str(self.home)

        if tty:
            return _run_on_a_terminal(
                argv, command, full_env, directory, stdin, timeout
            )

        completed = subprocess.run(
            command,
            env=full_env,
            cwd=directory,
            input=stdin,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return Run(
            args=argv,
            returncode=completed.returncode,
            stdout=completed.stdout,
            stderr=completed.stderr,
        )


def _run_on_a_terminal(argv, command, env, cwd, stdin, timeout):
    """Run the command with a pseudo-terminal on its standard streams.

    Needed because one behaviour is *defined* by whether a person is there: `grant` refuses a path
    the methodology does not declare, and offers to ask instead when stdin is a terminal. A pipe
    cannot exercise that branch -- under a pipe the code is required to refuse -- so the only way to
    test the question is to give the process something `[ -t 0 ]` agrees with.

    The two streams come back merged, because a terminal merges them: that is what the reader sees.
    """
    master, slave = pty.openpty()
    process = subprocess.Popen(
        command,
        stdin=slave,
        stdout=slave,
        stderr=slave,
        env=env,
        cwd=cwd,
        close_fds=True,
    )
    os.close(slave)
    if stdin:
        os.write(master, stdin.encode())

    chunks = []
    deadline = threading.Event()
    timer = threading.Timer(timeout, deadline.set)
    timer.start()
    try:
        while not deadline.is_set():
            ready, _, _ = select.select([master], [], [], 0.1)
            if not ready:
                if process.poll() is not None:
                    break
                continue
            try:
                chunk = os.read(master, 65536)
            except OSError:
                break
            if not chunk:
                break
            chunks.append(chunk)
    finally:
        timer.cancel()
        os.close(master)

    if deadline.is_set() and process.poll() is None:
        process.kill()
        process.wait()
        raise subprocess.TimeoutExpired(command, timeout)

    process.wait()
    output = b"".join(chunks).decode(errors="replace")
    return Run(args=argv, returncode=process.returncode, stdout=output, stderr="")


@pytest.fixture
def sealed(tmp_path, gpg_home):
    """An environment that belongs to this test alone: its own HOME, caches and keyring."""
    home = tmp_path / "home"
    home.mkdir(parents=True, exist_ok=True)
    cache_home = tmp_path / "cache"
    cache_home.mkdir(parents=True, exist_ok=True)
    config_home = tmp_path / "config"
    config_home.mkdir(parents=True, exist_ok=True)
    return Sealed(
        home=home, cache_home=cache_home, config_home=config_home, gnupghome=gpg_home
    )


@pytest.fixture
def run_validate(sealed):
    """Call the command under test, in an environment that belongs to this test alone.

    **Requesting this fixture marks a test `slow`.** It starts the command as a process, which is
    what the suite is for and also what makes it cost what it costs.
    """
    return Runner(sealed)


class BashLibrary:
    """Runs a snippet of bash with the command's own shell libraries sourced.

    The resolver is a shell library, and the reason it is one is that a user should be able to
    retype its steps at a prompt. A test that could only reach it through a subcommand would be
    testing the subcommand; this fixture calls the functions the way the documentation tells a
    reader to call them, which keeps the two honest about each other.
    """

    #: Sourced before every snippet, in the order `lib/utils` sources them: `sources.sh` needs
    #: `print_error`, and `paths.sh` needs the resolver.
    LIBRARIES = (
        "validation_utils.sh",
        "sources.sh",
        "page.sh",
        "paths.sh",
        "links.sh",
    )

    def __init__(self, sealed, lib_dir):
        self.sealed = sealed
        self.lib_dir = lib_dir

    def __call__(
        self,
        script,
        sources=None,
        offline=False,
        env=None,
        cwd=None,
        timeout=RUN_TIMEOUT,
    ):
        extra = dict(env or {})
        extra["FREEPORTS_VALIDATE_LIB"] = str(self.lib_dir)
        if sources is not None:
            joined = sources if isinstance(sources, str) else "\n".join(sources)
            extra["FREEPORTS_VALIDATE_SOURCES"] = joined
        if offline:
            extra["FREEPORTS_VALIDATE_OFFLINE"] = "1"
        # `LIB_DIR` because the libraries call each other's helpers by path, and `lib/utils` -- the
        # script that normally exports it -- is not sourced here: what these snippets exercise is a
        # library, not a subcommand.
        preamble = 'LIB_DIR="$FREEPORTS_VALIDATE_LIB"\n' + "".join(
            f'source "$FREEPORTS_VALIDATE_LIB/{library}"\n'
            for library in self.LIBRARIES
        )
        completed = subprocess.run(
            ["bash", "-c", preamble + script],
            env=self.sealed.environment(extra),
            cwd=str(cwd) if cwd is not None else str(self.sealed.home),
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return Run(
            args=["bash", "-c", script],
            returncode=completed.returncode,
            stdout=completed.stdout,
            stderr=completed.stderr,
        )


@pytest.fixture
def sources_lib(sealed):
    """Call `lib/sources.sh` directly, in the same sealed environment the command gets."""
    return BashLibrary(sealed, LIB_DIR)


# ---------------------------------------------------------------------------
# The Python halves of the library
# ---------------------------------------------------------------------------
#
# `lib/rst_paths.py` and `lib/pathmatch.py` are the two places where the bash/Python line falls the
# other way: parsing reStructuredText and matching `**` against a path are exactly the clever `awk`
# nobody could verify by reading, which is the property the rest of the command is shell for.
#
# They are *scripts* in a data directory, not modules of an importable package, so the suite loads
# them by path. Their behaviour is unit-tested that way -- an import is a hundred times cheaper than
# an interpreter start, and there are a lot of grammar cases to cover -- while a handful of tests
# run them as processes to hold the command-line contract that the shell scripts actually depend on.


def _load_lib_module(name):
    spec = importlib.util.spec_from_file_location(
        f"freeports_validate_lib_{name}", LIB_DIR / f"{name}.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.fixture(scope="session")
def rst_paths():
    """`lib/rst_paths.py`: what a methodology page declares about itself."""
    return _load_lib_module("rst_paths")


@pytest.fixture(scope="session")
def pathmatch():
    """`lib/pathmatch.py`: the pattern grammar of a `Supported paths` section."""
    return _load_lib_module("pathmatch")


@dataclass(frozen=True)
class LibScript:
    """One of the Python helpers, run the way the shell scripts run it."""

    name: str

    def __call__(self, *args, stdin="", timeout=RUN_TIMEOUT):
        completed = subprocess.run(
            ["python3", str(LIB_DIR / f"{self.name}.py"), *[str(a) for a in args]],
            input=stdin,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return Run(
            args=[self.name, *[str(a) for a in args]],
            returncode=completed.returncode,
            stdout=completed.stdout,
            stderr=completed.stderr,
        )


@pytest.fixture
def rst_paths_script():
    """`lib/rst_paths.py` as a process: the contract `lib/paths.sh` relies on."""
    return LibScript("rst_paths")


@pytest.fixture
def pathmatch_script():
    """`lib/pathmatch.py` as a process: the contract `lib/paths.sh` relies on."""
    return LibScript("pathmatch")


@pytest.fixture(scope="session")
def report():
    """`lib/report.py`: the renderings of the model `bin/collect` emits.

    Imported rather than run, because there are a great many rendering cases and each of them is a
    pure function of a dictionary -- no repository, no keyring and no network is involved in
    turning the model into text, which is the whole point of the model existing.
    """
    return _load_lib_module("report")


@pytest.fixture
def report_script():
    """`lib/report.py` as a process: the contract `bin/report` relies on."""
    return LibScript("report")


@pytest.fixture
def paths_lib(sealed):
    """Call `lib/paths.sh` directly -- the seam between the resolver and the two Python helpers."""
    return BashLibrary(sealed, LIB_DIR)


@pytest.fixture
def links_lib(sealed):
    """Call `lib/links.sh` directly -- resolving what a page cites, and checking what it pins."""
    return BashLibrary(sealed, LIB_DIR)


@pytest.fixture
def source_cache(sealed):
    """Where the resolver keeps the bodies it has fetched, for the tests that look inside it."""
    return sealed.cache_home / "freeports-validate"


@pytest.fixture
def signed_document(tmp_repo, signer, run_validate, local_source):
    """A created and signed validation document, which is the starting point of most flows.

    Returns its path. Written through the command rather than assembled here on purpose: a fixture
    that wrote the YAML itself would let `create-document` and `sign-document` break without a
    single test noticing, since every later test would still find the file it expected.
    """
    created = run_validate(
        "create-document",
        repo=tmp_repo,
        key_id=signer.fingerprint,
        sources=local_source,
    )
    assert created.returncode == 0, created
    signed = run_validate(
        "sign-document",
        repo=tmp_repo,
        key_id=signer.fingerprint,
        sources=local_source,
    )
    assert signed.returncode == 0, signed
    return tmp_repo.document(signer)
