"""The suite's own scaffolding, checked before anything is built on it.

Every other test file assumes a sealed environment, a throwaway keyring and a methodology tree
reachable two ways. Those assumptions are cheap to state and expensive to debug once a real test
fails for a reason that turns out to be a fixture -- so they are asserted here, once, in the file
whose failure means "the harness is wrong", not "the command is wrong".
"""

import subprocess
from hashlib import sha256
from urllib.request import urlopen


class TestSealedEnvironment:
    def test_the_shell_freeports_settings_do_not_leak_in(self, run_validate):
        env = run_validate.environment()
        assert not [key for key in env if key.startswith("FREEPORTS_")]

    def test_home_and_the_caches_point_inside_the_temporary_tree(self, run_validate):
        env = run_validate.environment()
        for key in ("HOME", "XDG_CACHE_HOME", "XDG_CONFIG_HOME", "GNUPGHOME"):
            assert env[key] != "", key
            assert env[key] not in (str(run_validate.home.parent.parent), "/"), key

    def test_named_settings_arrive_as_environment_variables(
        self, run_validate, tmp_repo
    ):
        env = run_validate.environment(
            {"FREEPORTS_FORMATS_REPO_PATH": str(tmp_repo.root)}
        )
        assert env["FREEPORTS_FORMATS_REPO_PATH"] == str(tmp_repo.root)


class TestTheCommandUnderTest:
    def test_usage_comes_from_the_source_tree(self, run_validate):
        run = run_validate()
        assert run.returncode == 1, run
        assert "Usage: freeports-validate" in run.output, run

    def test_help_exits_zero(self, run_validate):
        run = run_validate("--help")
        assert run.returncode == 0, run
        assert "Subcommands:" in run.output, run

    def test_an_unknown_subcommand_is_refused_by_name(self, run_validate):
        run = run_validate("no-such-subcommand")
        assert run.returncode == 1, run
        assert "no-such-subcommand" in run.output, run

    def test_a_terminal_run_reports_the_same_thing_as_a_piped_one(self, run_validate):
        piped = run_validate("--help")
        on_a_terminal = run_validate("--help", tty=True)
        assert on_a_terminal.returncode == piped.returncode
        assert "Subcommands:" in on_a_terminal.output


class TestKeyring:
    def test_the_signer_has_a_full_fingerprint(self, signer):
        assert len(signer.fingerprint) == 40
        assert signer.fingerprint == signer.fingerprint.upper()

    def test_the_two_identities_are_different_keys(self, signer, other_signer):
        assert signer.fingerprint != other_signer.fingerprint

    def test_the_document_name_is_the_one_the_command_derives(self, signer):
        assert signer.document_name == "test_granter.yaml"

    def test_the_key_signs_without_a_passphrase(self, signer, gpg_home):
        signed = subprocess.run(
            [
                "gpg",
                "--detach-sign",
                "--armor",
                "--default-key",
                signer.fingerprint,
                "-",
            ],
            input=b"a document",
            env={"GNUPGHOME": str(gpg_home), "PATH": "/usr/bin:/bin"},
            capture_output=True,
        )
        assert signed.returncode == 0, signed.stderr.decode()
        assert signed.stdout.startswith(b"-----BEGIN PGP SIGNATURE-----")


class TestRepository:
    def test_it_looks_like_a_formats_repository(self, tmp_repo):
        assert (tmp_repo.root / "metadata" / "formats.csv").is_file()
        assert tmp_repo.validation.is_dir()

    def test_the_output_layout_keeps_its_variant_level(self, tmp_repo):
        assert (tmp_repo.root / "tests/formats/FOO-EN24/1/out/funds.csv").is_file()

    def test_written_files_are_hashed_the_way_the_command_hashes_them(self, tmp_repo):
        path = tmp_repo.write("tests/formats/FOO-EN24/1/out/extra.csv", "a,b\n1,2\n")
        assert tmp_repo.sha256("tests/formats/FOO-EN24/1/out/extra.csv") == (
            sha256(path.read_bytes()).hexdigest()
        )


class TestMethodologySources:
    def test_the_tree_holds_the_pages_a_pattern_resolves_to(self, methodology_pages):
        assert methodology_pages.path("general_methodology").is_file()
        assert methodology_pages.path("methodologies/basic_check").is_file()

    def test_the_file_pattern_has_exactly_one_star(self, local_source):
        assert local_source.count("*") == 1
        assert local_source.startswith("file://")

    def test_a_page_is_served_over_http_byte_for_byte(self, http_source):
        served = urlopen(http_source.url("methodologies/basic_check"), timeout=5).read()
        assert served == http_source.tree.path("methodologies/basic_check").read_bytes()
        assert "/methodologies/basic_check.rst" in http_source.requested

    def test_a_stopped_server_refuses_the_connection(self, http_source):
        url = http_source.url("general_methodology")
        http_source.stop()
        fetched = subprocess.run(
            ["curl", "-fsS", "--max-time", "5", url], capture_output=True
        )
        assert fetched.returncode != 0

    def test_writing_a_page_changes_its_hash(self, methodology_pages):
        before = methodology_pages.sha256("methodologies/basic_check")
        methodology_pages.write(
            "methodologies/basic_check", "Basic check\n===========\n"
        )
        assert methodology_pages.sha256("methodologies/basic_check") != before


class TestDocumentFlow:
    def test_the_document_is_created_and_signed(self, signed_document, signer):
        assert signed_document.is_file()
        text = signed_document.read_text()
        assert signer.fingerprint in text
        assert "-----BEGIN PGP SIGNATURE-----" in text

    def test_it_passes_the_command_s_own_check(
        self, run_validate, tmp_repo, signer, signed_document, local_source
    ):
        run = run_validate(
            "check-grants",
            repo=tmp_repo,
            key_id=signer.fingerprint,
            sources=local_source,
        )
        assert run.returncode == 0, run
