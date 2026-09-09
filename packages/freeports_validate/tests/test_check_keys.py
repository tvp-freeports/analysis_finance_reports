"""Whether the keys behind this repository's grants are published where a stranger can fetch them.

`check-grants` verifies a signature against a keyring. That answers "was this document signed by
the key it names". It does not answer "can anybody else get that key" — and if they cannot, the
signature is checkable only by people who already have it, which is everybody except the person a
published grant exists to convince.

**Three answers, not two**, for the same reason `check-grants` has three. A 200 carrying an armoured
block is published. A 404 is not published. A network failure is *unknown*, and reporting it as
either would be a lie: as a pass it launders "I could not reach the server" into "the key is there",
and as a failure it turns somebody's train journey into a broken repository. That third state is the
whole reason this file is longer than a single happy-path test.

Nothing here reaches the real internet. The key server is a `http.server` on the loopback interface
at port zero — the same pattern `http_source` already uses in this suite — so two runs in parallel
cannot collide and an offline machine runs the tests exactly as an online one does.
"""

import os
import subprocess
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest


#: What a key server answers with when it has the key. Only the armour header is load-bearing.
ARMOURED_KEY = "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\nmDMEZAAAAA==\n-----END PGP PUBLIC KEY BLOCK-----\n"

#: The exit status the family uses for "nothing failed, and not everything was looked at".
CHECK_INCONCLUSIVE = 3

PACKAGE = Path(__file__).resolve().parents[1] / "src" / "freeports_validate"


class _KeyServer(BaseHTTPRequestHandler):
    """Answers a `pks/lookup` the way the setting for this test says to."""

    published = set()
    served_html_for_anything = False

    def do_GET(self):
        if self.served_html_for_anything:
            body = b"<html>No results found</html>"
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        fingerprint = self.path.rpartition("search=0x")[2].split("&")[0].upper()
        if fingerprint in self.published:
            body = ARMOURED_KEY.encode()
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_error(404, "Not found")

    def log_message(self, *args):
        pass


@pytest.fixture(scope="module")
def _keyserver_process():
    """One server for the whole module.

    Module-scoped because starting one is not free and nothing below needs its own. What each test
    varies is which fingerprints the server holds, and that is a class attribute reset between
    tests, not a reason to start another process.

    The poll interval is given rather than left at its default for the reason `http_source` in
    `conftest.py` gives it: `shutdown()` waits for `serve_forever`'s loop to look between `select`
    timeouts, so the default half-second is paid in full at every teardown. Module scope used to be
    the workaround for that; it is kept because it is right on its own terms, and the half-second
    is now gone as well.
    """

    class Handler(_KeyServer):
        published = set()
        served_html_for_anything = False

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(
        target=server.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True
    )
    thread.start()
    host, port = server.server_address[:2]
    Handler.url = f"http://{host}:{port}"
    yield Handler
    server.shutdown()
    server.server_close()
    thread.join(timeout=5)


@pytest.fixture
def keyserver(_keyserver_process):
    """The shared server, with its answers reset so no test inherits another's setup."""
    _keyserver_process.published = set()
    _keyserver_process.served_html_for_anything = False
    return _keyserver_process


@pytest.fixture
def repo(tmp_path):
    """A repository with a `validation/` directory and nothing else that matters here."""
    (tmp_path / "validation").mkdir()
    return tmp_path


def add_granter(repo, name, fingerprint):
    slug = name.lower().replace(" ", "_")
    (repo / "validation" / f"{slug}.yaml").write_text(
        f'version: abc\nwho:\n  name: "{name}"\n  email: a@b.c\n'
        f"  pubkey_id: {fingerprint}\nmethodologies: []\ndata: []\n"
    )


def run_check_keys(repo, keyserver_url, offline=False):
    """Run the subcommand the way `cli.py` hands over to it: a bash script and an environment."""
    env = dict(os.environ)
    env["FREEPORTS_VALIDATE_LIB"] = str(PACKAGE / "lib")
    env["FREEPORTS_FORMATS_REPO_PATH"] = str(repo)
    env["FREEPORTS_VALIDATE_KEYSERVER"] = keyserver_url
    env["FREEPORTS_VALIDATE_SOURCES"] = ""
    env.pop("FREEPORTS_VALIDATE_KEY_ID", None)
    if offline:
        env["FREEPORTS_VALIDATE_OFFLINE"] = "1"
    else:
        env.pop("FREEPORTS_VALIDATE_OFFLINE", None)
    return subprocess.run(
        [str(PACKAGE / "bin" / "check-keys")],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
    )


A = "09E506DD865A64A364B9F55610FF6836D9E35209"
B = "1111111111111111111111111111111111111111"


class TestAKeyThatIsPublished:
    def test_the_run_passes(self, repo, keyserver):
        keyserver.published = {A}
        add_granter(repo, "Oreste Sciacqualegni", A)
        assert run_check_keys(repo, keyserver.url).returncode == 0

    def test_the_granter_is_named_by_the_name_they_wrote(self, repo, keyserver):
        """`tr -d` once ate the space and turned this into "OresteSciacqualegni"."""
        keyserver.published = {A}
        add_granter(repo, "Oreste Sciacqualegni", A)
        assert "Oreste Sciacqualegni" in run_check_keys(repo, keyserver.url).stdout

    def test_the_count_is_reported(self, repo, keyserver):
        keyserver.published = {A, B}
        add_granter(repo, "One", A)
        add_granter(repo, "Two", B)
        assert "published 2/2" in run_check_keys(repo, keyserver.url).stdout


class TestAKeyThatIsNotPublished:
    def test_the_run_fails(self, repo, keyserver):
        keyserver.published = set()
        add_granter(repo, "One", A)
        assert run_check_keys(repo, keyserver.url).returncode == 1

    def test_the_fingerprint_that_is_missing_is_named(self, repo, keyserver):
        keyserver.published = set()
        add_granter(repo, "One", A)
        assert A in run_check_keys(repo, keyserver.url).stderr

    def test_the_message_says_how_to_publish_it(self, repo, keyserver):
        keyserver.published = set()
        add_granter(repo, "One", A)
        assert "--send-keys" in run_check_keys(repo, keyserver.url).stderr

    def test_one_missing_among_several_still_fails(self, repo, keyserver):
        keyserver.published = {A}
        add_granter(repo, "One", A)
        add_granter(repo, "Two", B)
        done = run_check_keys(repo, keyserver.url)
        assert done.returncode == 1
        assert "published 1/2" in done.stdout

    def test_a_server_answering_a_page_that_is_not_a_key_is_not_a_pass(
        self, repo, keyserver
    ):
        """A 200 is not enough: a "no results" page is a 200 too. The armour header settles it."""
        keyserver.served_html_for_anything = True
        add_granter(repo, "One", A)
        assert run_check_keys(repo, keyserver.url).returncode == 1


class TestAKeyNobodyCouldCheck:
    """Neither a pass nor a failure. Nothing says the key is missing — it says nobody looked."""

    def test_an_unreachable_server_is_inconclusive_rather_than_a_failure(self, repo):
        add_granter(repo, "One", A)
        # Port 1 on loopback: nothing listens there, so the request is refused rather than answered.
        done = run_check_keys(repo, "http://127.0.0.1:1")
        assert done.returncode == CHECK_INCONCLUSIVE

    def test_and_it_is_not_a_pass_either(self, repo):
        add_granter(repo, "One", A)
        assert run_check_keys(repo, "http://127.0.0.1:1").returncode != 0

    def test_the_message_says_nobody_looked(self, repo):
        add_granter(repo, "One", A)
        assert "nobody looked" in run_check_keys(repo, "http://127.0.0.1:1").stderr

    def test_offline_asks_nothing_and_says_so(self, repo, keyserver):
        keyserver.published = {A}
        add_granter(repo, "One", A)
        done = run_check_keys(repo, keyserver.url, offline=True)
        assert done.returncode == CHECK_INCONCLUSIVE
        assert "offline" in done.stderr

    def test_a_document_naming_no_key_is_unknown_not_missing(self, repo, keyserver):
        (repo / "validation" / "nameless.yaml").write_text(
            'version: abc\nwho:\n  name: "Nobody"\n  email: a@b.c\nmethodologies: []\ndata: []\n'
        )
        done = run_check_keys(repo, keyserver.url)
        assert done.returncode == CHECK_INCONCLUSIVE

    def test_a_failure_outweighs_an_unknown(self, repo, keyserver):
        """A key that is definitely absent is news; one nobody could check is not news enough to
        hide it."""
        keyserver.published = set()
        add_granter(repo, "One", A)
        (repo / "validation" / "nameless.yaml").write_text(
            'version: abc\nwho:\n  name: "Nobody"\n  email: a@b.c\nmethodologies: []\ndata: []\n'
        )
        assert run_check_keys(repo, keyserver.url).returncode == 1


class TestARepositoryWithNothingToCheck:
    def test_no_validation_directory_at_all_is_an_error(self, tmp_path, keyserver):
        assert run_check_keys(tmp_path, keyserver.url).returncode == 1

    def test_an_empty_validation_directory_passes_rather_than_failing(
        self, repo, keyserver
    ):
        """A repository nobody has granted anything in has no unpublished key in it."""
        done = run_check_keys(repo, keyserver.url)
        assert done.returncode == 0
